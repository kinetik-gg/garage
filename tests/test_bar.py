"""The [bar] section, from the schema table down to the two files it writes.

The bar is the one surface whose *layout* is a preference. Everything else in the
schema reaches a colour, a timeout or a compositor option; height, the widget
toggles, the padding scale and the background reach a waybar option and a GTK
rule -- and waybar's own merge rule makes that awkward in a way worth pinning:

  * an option named in ~/.config/waybar/config.jsonc is won by that file, and an
    include can only supply an option the naming file left out. So height,
    modules-left, modules-center and modules-right have to be absent from
    config.jsonc for the fragment to decide them at all. That absence is a
    property of a *tracked* file, and nothing but a test will notice it coming
    back;
  * GTK CSS has no arithmetic, so a scaled padding is a generated padding. The
    base sheet must therefore name none of the values in PADDING_TABLE, or the
    scale would be silently overridden for whichever one it kept.

Both are checked here against the checked-in files, not only against the
renderers. The rest is the toggle matrix, which is the part a user can reach: six
metric switches, the AI strip and the media readout, over a right side whose
static tail must survive all of them.
"""

from __future__ import annotations

import copy
import inspect
import json
import os
import re
import runpy
import subprocess
import tempfile
import unittest
from pathlib import Path

from harness import BackendTestCase


REPO_ROOT = Path(__file__).resolve().parent.parent
WAYBAR = REPO_ROOT / "desktop" / ".config" / "waybar"
CONFIG_JSONC = WAYBAR / "config.jsonc"
BASE_CSS = WAYBAR / "waybar-base.css"
SHELL_DIR = REPO_ROOT / "desktop" / ".config" / "quickshell" / "garage"
MEDIA_PALETTE_QML = SHELL_DIR / "MediaPalette.qml"
MEDIA_CARD_QML = SHELL_DIR / "MediaCard.qml"
SHELL_QML = SHELL_DIR / "shell.qml"

# The scripts this wave superseded. garage-metrics answers for all four now, and
# a config.jsonc that still names one would point the bar at a file that is gone.
RETIRED_SCRIPTS = ("system-activity.py", "gpu-activity.py", "disk-activity.py",
                   "activity-graph.py")

# The options config.jsonc must not name, because the generated fragments decide
# them. Naming one here would not fail loudly: the bar would simply keep this
# file's answer and every toggle in the pane would appear to do nothing.
FRAGMENT_OWNED_OPTIONS = ("height", "modules-left", "modules-center", "modules-right")


def strip_comments(text: str) -> str:
    """JSONC to JSON, well enough to parse this file.

    Line comments only, which is all config.jsonc uses. Naive about `//` inside a
    string, which is safe here rather than assumed safe: a value containing one
    would be mangled into invalid JSON and the parse test below would say so.
    """
    return re.sub(r"//[^\n]*", "", text)


class ShippedBarConfig(unittest.TestCase):
    """The tracked waybar files, which no renderer can correct."""

    def setUp(self) -> None:
        self.config_text = CONFIG_JSONC.read_text(encoding="utf-8")
        self.config = json.loads(strip_comments(self.config_text))

    def test_the_shipped_config_is_parseable_json_once_comments_are_stripped(self) -> None:
        # waybar's own parser takes the comments; this is the check that the file
        # is otherwise well formed, because a trailing comma or a stray brace
        # leaves the bar with no config at all and only the journal says why.
        self.assertIsInstance(self.config, dict)

    def test_config_names_none_of_the_options_the_fragments_publish(self) -> None:
        for option in FRAGMENT_OWNED_OPTIONS:
            with self.subTest(option=option):
                self.assertNotIn(option, self.config,
                                 f"config.jsonc names {option!r}; waybar gives a "
                                 "named option to the naming file, so the "
                                 "generated fragment could never override it")

    def test_config_includes_all_three_fragments(self) -> None:
        includes = [Path(entry).name for entry in self.config["include"]]
        self.assertEqual(["waybar-clock.jsonc", "waybar-workspaces.jsonc",
                          "waybar-widgets.jsonc"], includes)

    def test_config_names_no_retired_script(self) -> None:
        for script in RETIRED_SCRIPTS:
            with self.subTest(script=script):
                self.assertNotIn(script, self.config_text)
                self.assertFalse((WAYBAR / script).exists(),
                                 "the script is gone from the config but still "
                                 "in the checkout")

    def test_the_media_module_is_defined_by_the_fragment_and_not_here(self) -> None:
        # Both halves have to move together: bar.media_player decides whether the
        # module is listed, and a definition left behind here would be a second
        # writer of the same module with a different on-click.
        self.assertNotIn("custom/media", self.config)

    def test_the_image_module_recipe_is_documented_as_working(self) -> None:
        # The comment is load-bearing: it is the only record that `path` spins the
        # bar at 100% CPU, and the next person to reach for the obvious spelling
        # reads it here. Pinned so the warning cannot be dropped in a tidy-up.
        for phrase in ("image module", "100% CPU", "interval"):
            with self.subTest(phrase=phrase):
                self.assertIn(phrase, self.config_text)


class BaseSheetOwnsNoSpacing(BackendTestCase):
    """PADDING_TABLE's values are generated, so the base sheet may name none."""

    def test_no_padding_or_margin_declaration_survives_in_the_base_sheet(self) -> None:
        css = BASE_CSS.read_text(encoding="utf-8")
        # Comments carry the reasoning and mention px; the declarations are what
        # matters, so only real `padding:`/`margin:` lines are read.
        declarations = [line.strip() for line in css.splitlines()
                        if re.match(r"(padding|margin)(-\w+)?\s*:", line.strip())]
        # `padding: 0` on #workspaces is not spacing: it is the container's own
        # zero, and the scale has nothing to say about zero.
        offending = [line for line in declarations if not re.fullmatch(
            r"(padding|margin)(-\w+)?\s*:\s*0\s*;", line)]
        self.assertEqual([], offending,
                         "a spacing declaration here is one bar.padding_scale "
                         "cannot reach -- move its base value into PADDING_TABLE")

    def test_every_padding_table_entry_reaches_the_generated_sheet(self) -> None:
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        css = self.garage.waybar_spacing_css(config)
        scale = config["bar"]["padding_scale"]
        for name, base in self.garage.PADDING_TABLE.items():
            with self.subTest(entry=name):
                self.assertIn(f"{round(base * scale)}px", css,
                              "an entry nothing renders is a value that silently "
                              "stopped applying")


class BarPanelToggle(unittest.TestCase):
    """Bar clicks reach their output-local anchors even with stale state."""

    TOGGLE = REPO_ROOT / "desktop" / ".local" / "bin" / "garage-panel-toggle"

    def fake_desktop(self, root: Path, cursor_x: int = 150) -> tuple[Path, Path]:
        bin_dir = root / "bin"
        capture = root / "qs-call"
        bin_dir.mkdir()
        hyprctl = bin_dir / "hyprctl"
        hyprctl.write_text(
            "#!/bin/sh\n"
            "case \"$1\" in\n"
            f"  cursorpos) printf '%s\\n' '{{\"x\":{cursor_x},\"y\":10}}' ;;\n"
            "  monitors) printf '%s\\n' "
            "'[{\"name\":\"DP-1\",\"x\":0,\"y\":0,"
            "\"width\":1920,\"height\":1080,\"scale\":1}]' ;;\n"
            "  workspaces) printf '%s\\n' "
            "'[{\"name\":\"1\",\"monitor\":\"DP-1\"},"
            "{\"name\":\"2\",\"monitor\":\"DP-1\"},"
            "{\"name\":\"3\",\"monitor\":\"DP-1\"},"
            "{\"name\":\"4\",\"monitor\":\"DP-1\"}]' ;;\n"
            "esac\n",
            encoding="utf-8")
        qs = bin_dir / "qs"
        qs.write_text(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$CAPTURE\"\n",
            encoding="utf-8")
        hyprctl.chmod(0o755)
        qs.chmod(0o755)
        return bin_dir, capture

    def run_toggle(self, root: Path, *arguments: str, cursor_x: int = 150) -> str:
        bin_dir, capture = self.fake_desktop(root, cursor_x)
        env = os.environ | {
            "PATH": f"{bin_dir}:/usr/bin",
            "HOME": str(root),
            "XDG_STATE_HOME": str(root / "state"),
            "XDG_CONFIG_HOME": str(root / "config"),
            "CAPTURE": str(capture),
        }
        subprocess.run([str(self.TOGGLE), *arguments], env=env,
                       check=True, capture_output=True, text=True)
        return capture.read_text(encoding="utf-8")

    def desktop_state(self, root: Path, fragment: str, stylesheet: str) -> None:
        """The two generated files the anchor is computed out of."""
        generated = root / "state" / "garage" / "generated"
        waybar = root / "config" / "waybar"
        generated.mkdir(parents=True)
        waybar.mkdir(parents=True)
        (generated / "waybar-widgets.jsonc").write_text(fragment, encoding="utf-8")
        (waybar / "style.css").write_text(stylesheet, encoding="utf-8")

    def test_the_anchor_is_the_centre_of_the_whole_metric_group(self) -> None:
        # Two strips of 100 with 12px of padding a side: a 248px group ending
        # 300px in from the right edge of a 1920px output. Its centre is
        # 1920 - 300 - 124. Both numbers come out of the generated files rather
        # than out of the script, which is the property this pins.
        with tempfile.TemporaryDirectory() as root_text:
            root = Path(root_text)
            self.desktop_state(root, json.dumps({
                "group/monitoring": {
                    "modules": ["image#metric-cpu", "image#metric-gpu"],
                },
                "image#metric-cpu": {"size": 100},
                "image#metric-gpu": {"size": 100},
            }), "#image {\n  padding: 0 12px;\n}\n"
                "#status-tail {\n  min-width: 300px;\n}\n")
            call = self.run_toggle(root, "monitor", "cpu")
        self.assertEqual("-c garage ipc call shell monitorOn DP-1 1496\n", call)

    def test_the_click_lands_in_the_same_place_whichever_strip_it_was(self) -> None:
        """The reason no cursor coordinate reaches the anchor."""
        fragment = json.dumps({
            "group/monitoring": {
                "modules": ["image#metric-cpu", "image#metric-gpu"],
            },
            "image#metric-cpu": {"size": 100},
            "image#metric-gpu": {"size": 100},
        })
        stylesheet = ("#image {\n  padding: 0 12px;\n}\n"
                      "#status-tail {\n  min-width: 300px;\n}\n")
        anchors = set()
        for widget in ("cpu", "gpu"):
            with tempfile.TemporaryDirectory() as root_text:
                root = Path(root_text)
                self.desktop_state(root, fragment, stylesheet)
                anchors.add(self.run_toggle(root, "monitor", widget))
        self.assertEqual(1, len(anchors), anchors)

    def test_a_broken_widget_fragment_still_opens_on_the_shipped_defaults(self) -> None:
        # Runtime state a half-finished upgrade or a hand edit left unreadable.
        # Under `set -e` this is the difference between a slightly misplaced
        # dashboard and a bar click that does nothing at all.
        with tempfile.TemporaryDirectory() as root_text:
            root = Path(root_text)
            self.desktop_state(root, "not json\n", "")
            call = self.run_toggle(root, "monitor", "cpu")
        self.assertEqual("-c garage ipc call shell monitorOn DP-1 1600\n", call)

    def test_the_widgetless_command_is_still_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as root_text:
            call = self.run_toggle(Path(root_text), "monitor")
        self.assertEqual("-c garage ipc call shell monitorOn DP-1 1600\n", call)

    def test_media_uses_one_group_anchor_wherever_the_label_is_clicked(self) -> None:
        calls = set()
        for cursor_x in (180, 420):
            with tempfile.TemporaryDirectory() as root_text:
                calls.add(self.run_toggle(Path(root_text), "media",
                                          cursor_x=cursor_x))
        self.assertEqual({"-c garage ipc call shell mediaOn DP-1 347\n"}, calls)

    def test_an_unknown_widget_is_refused_before_querying_the_compositor(self) -> None:
        result = subprocess.run([str(self.TOGGLE), "monitor", "bogus"],
                                capture_output=True, text=True)
        self.assertNotEqual(0, result.returncode)
        self.assertIn("usage:", result.stderr)


class MediaPalettePlacement(unittest.TestCase):
    """The media click's anchor survives the IPC-to-surface handoff."""

    def test_the_shell_forwards_the_media_anchor(self) -> None:
        shell = SHELL_QML.read_text(encoding="utf-8")
        self.assertIn("property real mediaAnchorX: -1", shell)
        self.assertIn("targetAnchorX: shell.mediaAnchorX", shell)
        self.assertIn("function mediaOn(screenName: string, anchorX: int)", shell)
        self.assertIn("shell.mediaAnchorX = anchorX", shell)

    def test_the_surface_centres_on_the_anchor_and_clamps_to_the_output(self) -> None:
        palette = MEDIA_PALETTE_QML.read_text(encoding="utf-8")
        self.assertIn("required property real targetAnchorX", palette)
        self.assertIn("? media.targetAnchorX - media.implicitWidth / 2", palette)
        self.assertRegex(palette, r"anchors\s*\{\s*top: true\s*left: true\s*\}")
        self.assertIn("margins.left: media.surfaceLeft", palette)

    def test_the_click_calculation_uses_the_palettes_real_width(self) -> None:
        palette = MEDIA_PALETTE_QML.read_text(encoding="utf-8")
        script = BarPanelToggle.TOGGLE.read_text(encoding="utf-8")
        qml_width = int(re.search(r"implicitWidth:\s*(\d+)", palette).group(1))
        script_width = int(re.search(r"palette_width=(\d+)", script).group(1))
        self.assertEqual(qml_width, script_width)

    def test_playerctld_is_not_presented_as_a_second_real_player(self) -> None:
        for path in (MEDIA_PALETTE_QML, MEDIA_CARD_QML):
            with self.subTest(component=path.name):
                source = path.read_text(encoding="utf-8")
                self.assertIn('candidate.dbusName || ""', source)
                self.assertIn('endsWith(".playerctld")', source)


class MediaPaletteArtwork(unittest.TestCase):
    """The full-size cover remains a subtle, stable part of the glass."""

    def test_cover_is_blurred_cropped_and_masked_behind_the_controls(self) -> None:
        palette = MEDIA_PALETTE_QML.read_text(encoding="utf-8")
        self.assertIn("id: backdropArt", palette)
        self.assertIn("fillMode: Image.PreserveAspectCrop", palette)
        self.assertIn("retainWhileLoading: true", palette)
        self.assertIn("source: backdropArt", palette)
        self.assertIn("blurEnabled: true", palette)
        self.assertIn("sourceItem: backdropMask", palette)
        self.assertIn("hideSource: true", palette)
        self.assertIn("maskSource: backdropMaskTexture", palette)
        self.assertIn("opacity: 0.3", palette)


class MonitorAnchorIsOneNumber(BackendTestCase):
    """The tail width the bar lays out and the one the click reads are one value."""

    TOGGLE = REPO_ROOT / "desktop" / ".local" / "bin" / "garage-panel-toggle"

    def test_the_generated_sheet_publishes_the_tail_width(self) -> None:
        css = self.garage.waybar_spacing_css(
            copy.deepcopy(self.garage.FALLBACK_DEFAULTS))
        self.assertIn(f"min-width: {self.garage.WAYBAR_STATUS_TAIL_WIDTH}px", css,
                      "garage-panel-toggle reads this back out of the sheet")

    def test_the_scripts_fallback_is_the_same_number(self) -> None:
        # The script cannot import the constant, so the one copy it does carry --
        # for the case where the sheet is missing -- is checked against it here.
        script = self.TOGGLE.read_text(encoding="utf-8")
        self.assertIn(f"tail_width={self.garage.WAYBAR_STATUS_TAIL_WIDTH}\n", script)
        self.assertIn(f"tail_width=${{configured_tail:-"
                      f"{self.garage.WAYBAR_STATUS_TAIL_WIDTH}}}", script)


class BarGlyphs(unittest.TestCase):
    """The Phosphor glyphs the bar's strips and its AI module are made of.

    The strips label themselves with an icon rather than the words CPU / MEM /
    TEMP, and the AI module is one sparkle rather than four numbers. Both are
    couplings nothing else checks: a strip's icon name has to have path data in
    the collector *and* a house asset the popovers can draw, and the AI glyph is
    only a glyph if the module's font stack asks for Phosphor first -- otherwise
    Plus Jakarta Sans claims the private-use codepoint and the bar shows tofu.
    """

    METRICS = REPO_ROOT / "desktop" / ".local" / "bin" / "garage-metrics"
    AI_USAGE = REPO_ROOT / "desktop" / ".local" / "bin" / "garage-ai-usage"
    SHELL_ICONS = REPO_ROOT / "desktop" / ".config" / "quickshell" / "garage" / "icons"

    @classmethod
    def setUpClass(cls) -> None:
        cls.metrics = runpy.run_path(str(cls.METRICS))

    def literal(self, path: Path, name: str):
        """A module-level literal assignment, read without importing the script."""
        import ast
        for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
            if (isinstance(node, ast.Assign)
                    and any(isinstance(t, ast.Name) and t.id == name
                            for t in node.targets)):
                return ast.literal_eval(node.value)
        self.fail(f"{name} not found in {path.name}")

    def test_every_strip_icon_has_path_data_and_a_house_asset(self) -> None:
        layouts = self.literal(self.METRICS, "LAYOUTS")
        icons = self.literal(self.METRICS, "ICON_PATHS")
        for widget, layout in layouts.items():
            if "icon" not in layout:
                self.assertEqual("network", widget,
                                 "only self-labelled down/up figures omit a glyph")
                continue
            with self.subTest(widget=widget):
                name = layout["icon"]
                self.assertIn(name, icons,
                              "the strip names an icon the collector cannot draw")
                asset = self.SHELL_ICONS / f"{name}.svg"
                self.assertTrue(asset.exists(),
                                f"{name}.svg is missing from the shell's icons, so "
                                "the popovers cannot draw the strip's own glyph")
                svg = asset.read_text(encoding="utf-8")
                # Same coordinates in both places, or the transform in
                # render_widget() would be scaling from the wrong box.
                self.assertIn('viewBox="0 0 256 256"', svg)
                self.assertIn(icons[name], svg,
                              "the asset and the embedded path have drifted apart")

    def test_the_network_strip_is_the_one_with_no_graph(self) -> None:
        # The owner's call, and the reason its width is what it is: two throughput
        # figures and no line. render_widget() keys off the absence of the width.
        layouts = self.literal(self.METRICS, "LAYOUTS")
        self.assertNotIn("graph_width", layouts["network"])
        self.assertNotIn("icon", layouts["network"],
                         "the readings already carry individual down/up arrows")
        for widget in ("cpu", "memory", "temp", "disk", "gpu"):
            with self.subTest(widget=widget):
                self.assertIn("graph_width", layouts[widget])

    def test_strip_text_is_vertically_centred_with_its_icon(self) -> None:
        svg = self.metrics["render_svg"]("cpu", {
            "history": [5], "display": "5%", "active": False
        })
        self.assertIn("dominant-baseline: central", svg)
        self.assertIn('<text x="48" y="11">5%</text>', svg)

    def test_network_uses_two_uniform_direction_glyphs_and_no_third_icon(self) -> None:
        svg = self.metrics["render_svg"]("network", {
            "display": "14K", "extra": "1K", "active": False
        })
        scale = self.metrics["ICON_SIZE"] / 256
        self.assertIn(f'translate(0 4) scale({scale:.6f})', svg)
        self.assertIn(f'translate(68 4) scale({scale:.6f})', svg)
        self.assertIn('<text x="20" y="11">14K</text>', svg)
        self.assertIn('<text x="88" y="11" class="secondary">1K</text>', svg)
        self.assertNotIn("↓", svg)
        self.assertNotIn("↑", svg)

    def test_a_percentage_is_plotted_where_the_percentage_says(self) -> None:
        """The axis is linear, and that is the whole of it.

        It was log1p for a while, on the theory that idle activity needs lifting
        to be visible. What it actually did was put 1% of load at 15% of the
        graph's height and 20% at 66%, so a card near idle drew a line thrashing
        across the strip and a 45C package sat just under the top of its plot.
        """
        clamp = self.metrics["_clamp_sample"]
        self.assertEqual(0, clamp(0))
        self.assertEqual(100, clamp(100))
        self.assertEqual(45, clamp(45))
        self.assertEqual(100, clamp(140), "a reading over full scale is clamped")
        self.assertEqual(0, clamp("not a sample"))

        # And the same, as geometry: half load is half the height of the plot.
        top, bottom = self.metrics["GRAPH_TOP"], self.metrics["GRAPH_BOTTOM"]
        points = self.metrics["_coordinates"]([50] * 8, 0, 22)
        self.assertAlmostEqual((top + bottom) / 2, points[0][1], places=6)
        self.assertNotIn("history_log_scaled", self.metrics["LAYOUTS"]["disk"],
                         "nothing is scaled twice because nothing is scaled here")

    def test_a_strip_plots_about_one_point_per_pixel_of_graph(self) -> None:
        """POINTS is 120 and a graph is 22px wide.

        Drawn straight, that is five vertices per pixel column -- no more detail
        than their mean, just more ink, and on a volatile series it renders as a
        vertical hash instead of a line.
        """
        width = self.metrics["LAYOUTS"]["gpu"]["graph_width"]
        points = self.metrics["_coordinates"](list(range(120)), 20, width)
        self.assertLessEqual(len(points), round(width) + 1)
        self.assertGreaterEqual(len(points), round(width) / 2)
        # A history shorter than the graph is wide is left alone.
        self.assertEqual(6, len(self.metrics["_coordinates"]([1] * 6, 20, width)))

    def test_the_reduction_is_the_mean_rather_than_the_peak(self) -> None:
        """A strip of bucket maxima reads as a machine permanently at full tilt."""
        resample = self.metrics["_resample"]
        self.assertEqual([1.5, 3.5], resample([1.0, 2.0, 3.0, 4.0], 2))
        # The peak of the first bucket is 100; its mean is 50.
        self.assertEqual([50.0, 0.0], resample([0.0, 100.0, 0.0, 0.0], 2))
        # One column is not a line, so the history is left as it is.
        self.assertEqual([0.0, 100.0], resample([0.0, 100.0], 1))

    def test_the_ai_module_is_one_private_use_glyph_and_asks_for_phosphor(self) -> None:
        sparkle = self.literal(self.AI_USAGE, "SPARKLE")
        self.assertEqual(1, len(sparkle), "the AI module is a single glyph")
        self.assertTrue(0xE000 <= ord(sparkle) <= 0xF8FF,
                        "an icon font glyph lives in the private use area")
        source = self.AI_USAGE.read_text(encoding="utf-8")
        self.assertNotIn("OAI {", source,
                         "the OAI/ANT summary belongs in the tooltip, not the bar")
        rule = re.search(r"#custom-ai-usage \{([^}]*)\}",
                         BASE_CSS.read_text(encoding="utf-8"))
        self.assertIsNotNone(rule, "the AI module has no font rule of its own")
        family = re.search(r"font-family:\s*([^;]+);", rule.group(1))
        self.assertTrue(family.group(1).strip().startswith('"Phosphor"'),
                        "Phosphor has to come first or another font claims the "
                        f"codepoint: {family.group(1)}")


class BarWidgetFragment(BackendTestCase):
    """The toggle matrix, read off the fragment the bar actually loads."""

    def fragment(self, **departures) -> dict:
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        config["bar"].update(departures)
        self.garage.render_bar_widgets(config)
        return json.loads(self.garage.WAYBAR_WIDGETS.read_text(encoding="utf-8"))

    def all_monitors(self, value: bool) -> dict:
        return {f"monitor_{name}": value for name in self.garage.BAR_METRICS}

    def monitoring(self, fragment: dict) -> list[str]:
        return fragment[self.garage.WAYBAR_MONITOR_GROUP]["modules"]

    def test_the_shipped_defaults_carry_the_three_universal_strips(self) -> None:
        # cpu, memory and network are on by default; temperature, disk and GPU are
        # not, because each depends on hardware garage-metrics may find nothing
        # for. Asserted as the whole list so an added default is a decision.
        fragment = self.fragment()
        right = fragment["modules-right"]
        self.assertEqual([self.garage.WAYBAR_MONITOR_GROUP,
                          self.garage.WAYBAR_STATUS_GROUP], right)
        self.assertEqual(["image#metric-cpu", "image#metric-memory",
                          "image#metric-network"],
                         self.monitoring(fragment))
        self.assertEqual(["custom/ai-usage", *self.garage.WAYBAR_MODULES_RIGHT],
                         fragment[self.garage.WAYBAR_STATUS_GROUP]["modules"])

    def test_every_strip_on_is_bar_metrics_order(self) -> None:
        """The order is the table's, not the order the toggles were flipped in."""
        fragment = self.fragment(**self.all_monitors(True))
        self.assertEqual([f"image#metric-{name}" for name in self.garage.BAR_METRICS],
                         self.monitoring(fragment)[:len(self.garage.BAR_METRICS)])

    def test_with_everything_off_the_static_tail_is_still_the_whole_right_side(self) -> None:
        """The property the fragment exists for.

        config.jsonc no longer names modules-right, so this list is the only thing
        that puts the bell, the launcher, the control centre and the clock on the
        bar. A renderer that emitted nothing when every widget was off would take
        them with it.
        """
        fragment = self.fragment(ai_usage=False, media_player=False,
                                 **self.all_monitors(False))
        self.assertEqual([self.garage.WAYBAR_MONITOR_GROUP,
                          self.garage.WAYBAR_STATUS_GROUP], fragment["modules-right"])
        self.assertEqual([], self.monitoring(fragment))
        self.assertEqual(self.garage.WAYBAR_MODULES_RIGHT,
                         fragment[self.garage.WAYBAR_STATUS_GROUP]["modules"])
        self.assertEqual([], fragment["modules-center"])

    def test_the_static_tail_is_last_however_many_widgets_are_on(self) -> None:
        for departures in ({}, self.all_monitors(True), self.all_monitors(False),
                           {"ai_usage": False}, {"monitor_gpu": True}):
            with self.subTest(bar=departures):
                fragment = self.fragment(**departures)
                tail = fragment[self.garage.WAYBAR_STATUS_GROUP]["modules"]
                self.assertEqual(self.garage.WAYBAR_MODULES_RIGHT,
                                 tail[-len(self.garage.WAYBAR_MODULES_RIGHT):])

    def left(self, **departures) -> dict:
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        config["bar"].update(departures)
        self.garage.render_bar_workspaces(config)
        return json.loads(self.garage.WAYBAR_WORKSPACES.read_text(encoding="utf-8"))

    def test_the_media_toggle_places_it_after_workspaces_and_flips_its_definition(self) -> None:
        on = self.fragment(media_player=True)
        self.assertEqual([], on["modules-center"])
        self.assertEqual([*self.garage.WAYBAR_MODULES_LEFT,
                          self.garage.WAYBAR_WORKSPACE_MODULE,
                          "custom/media"],
                         self.left(media_player=True)["modules-left"])
        self.assertIn("custom/media", on)
        self.assertEqual("$HOME/.local/bin/garage-panel-toggle media",
                         on["custom/media"]["on-click"])
        off = self.fragment(media_player=False)
        self.assertEqual([], off["modules-center"])
        self.assertNotIn("custom/media",
                         self.left(media_player=False)["modules-left"])
        self.assertNotIn("custom/media", off,
                         "a definition for a module nobody lists reads as though "
                         "its exec is still running every two seconds")

        without_workspaces = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        without_workspaces["workspaces"]["indicator"] = False
        self.garage.render_bar_workspaces(without_workspaces)
        left = json.loads(self.garage.WAYBAR_WORKSPACES.read_text(encoding="utf-8"))
        self.assertEqual([*self.garage.WAYBAR_MODULES_LEFT, "custom/media"],
                         left["modules-left"])

    def test_the_media_module_keeps_its_transport_controls(self) -> None:
        # Moved out of config.jsonc, so everything but on-click has to arrive
        # unchanged; the scroll wheel and middle click are the whole of it.
        media = self.fragment()["custom/media"]
        self.assertEqual("$HOME/.config/waybar/media-status.py", media["exec"])
        self.assertEqual("json", media["return-type"])
        self.assertTrue(media["hide-empty-text"])
        for key in ("on-click-middle", "on-click-right",
                    "on-scroll-up", "on-scroll-down"):
            with self.subTest(binding=key):
                self.assertIn("playerctl", media[key])

    def test_the_ai_strip_hides_itself_and_is_polled_in_minutes(self) -> None:
        on = self.fragment(ai_usage=True)
        self.assertIn("custom/ai-usage",
                      on[self.garage.WAYBAR_STATUS_GROUP]["modules"])
        module = on["custom/ai-usage"]
        self.assertEqual("json", module["return-type"])
        # The empty text garage-ai-usage prints without tokscale only hides the
        # module if the module asks it to.
        self.assertTrue(module["hide-empty-text"])
        self.assertGreaterEqual(module["interval"], 60,
                                "tokscale is a subprocess per tick and the figure "
                                "moves over a billing window, not a second")
        off = self.fragment(ai_usage=False)
        self.assertNotIn("custom/ai-usage",
                         off[self.garage.WAYBAR_STATUS_GROUP]["modules"])
        self.assertNotIn("custom/ai-usage", off)

    def test_every_metric_module_follows_the_proven_image_recipe(self) -> None:
        """exec + interval + size + tooltip, and no other spelling.

        The recipe is not a style choice. `path`-based image modules -- and every
        other form tried -- spin waybar at 100% CPU with no bar ever drawn, and an
        `exec` with no `interval` deadlocks construction. This is the form that
        was probed working live, so the renderer may emit only these keys.
        """
        fragment = self.fragment(**self.all_monitors(True))
        for name in self.garage.BAR_METRICS:
            module = fragment[f"image#metric-{name}"]
            with self.subTest(widget=name):
                self.assertEqual({"exec", "size", "interval", "tooltip", "on-click"},
                                 set(module))
                self.assertEqual(f"$HOME/.local/bin/garage-metrics --bar-svg {name}",
                                 module["exec"])
                self.assertEqual(2, module["interval"])
                self.assertTrue(module["tooltip"])
                self.assertEqual(
                    f"$HOME/.local/bin/garage-panel-toggle monitor {name}",
                    module["on-click"])
                self.assertNotIn("path", module)

    def test_the_metric_names_are_the_collectors_own_widgets(self) -> None:
        # BAR_METRICS, the bar.monitor_* keys and garage-metrics' WIDGETS are one
        # list. The first two are checkable here; the third is the exec above.
        for name in self.garage.BAR_METRICS:
            with self.subTest(widget=name):
                self.assertIn(f"bar.monitor_{name}", self.garage.PREFERENCE_SCHEMA)

    def test_the_height_is_published_and_the_strip_sizes_are_the_svg_widths(self) -> None:
        # waybar's image `size` boxes the LARGEST dimension: an 82x22 strip at
        # size 22 renders 22px wide (measured -- the strips shrank to
        # illegibility). Natural rendering means size == the SVG's width, per
        # widget, independent of bar height.
        for height in (30, 36, 43, 60):
            with self.subTest(height=height):
                fragment = self.fragment(height=height, **self.all_monitors(True))
                self.assertEqual(height, fragment["height"])
                for name in self.garage.BAR_METRICS:
                    self.assertEqual(self.garage.METRIC_STRIP_WIDTHS[name],
                                     fragment[f"image#metric-{name}"]["size"])

    def test_the_strip_widths_match_the_collector(self) -> None:
        """METRIC_STRIP_WIDTHS mirrors LAYOUTS[...]["width"] in garage-metrics.

        The two scripts cannot import each other, so the pin is enforced here:
        parse the collector's LAYOUTS table and fail on any drift.
        """
        import ast
        source = (REPO_ROOT / "desktop" / ".local" / "bin" / "garage-metrics").read_text()
        tree = ast.parse(source)
        layouts = None
        for node in ast.walk(tree):
            if (isinstance(node, ast.Assign)
                    and any(isinstance(t, ast.Name) and t.id == "LAYOUTS"
                            for t in node.targets)):
                layouts = ast.literal_eval(node.value)
        self.assertIsNotNone(layouts, "LAYOUTS table not found in garage-metrics")
        self.assertEqual(
            {name: spec["width"] for name, spec in layouts.items()},
            self.garage.METRIC_STRIP_WIDTHS)

    def test_the_fragment_is_rewritten_whole_rather_than_merged(self) -> None:
        # One writer per fragment. A renderer that added to what it found would
        # keep a strip the user has just switched off.
        self.assertIn("image#metric-gpu", self.fragment(monitor_gpu=True))
        self.assertNotIn("image#metric-gpu", self.fragment(monitor_gpu=False))

    def test_the_fragment_is_written_atomically_and_parses(self) -> None:
        # The generated directory need not exist first: render_bar_widgets()
        # creates it, because `garage render-bar` runs on a first boot.
        self.assertFalse(self.garage.GENERATED.exists())
        self.fragment()
        text = self.garage.WAYBAR_WIDGETS.read_text(encoding="utf-8")
        self.assertTrue(text.endswith("\n"))
        json.loads(text)
        siblings = [path.name for path in self.garage.WAYBAR_WIDGETS.parent.iterdir()
                    if path.name.startswith(".waybar-widgets")]
        self.assertEqual([], siblings, "an atomic_write() temporary was left behind")


class BarPaddingScale(BackendTestCase):
    """The scale, against PADDING_TABLE's base values."""

    def spacing(self, scale: float, height: int = 43) -> str:
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        config["bar"]["padding_scale"] = scale
        config["bar"]["height"] = height
        return self.garage.waybar_spacing_css(config)

    def pixels(self, css: str) -> list[int]:
        return [int(value) for value in re.findall(r"(\d+)px", css)]

    def test_scale_one_is_exactly_the_shipped_table(self) -> None:
        css = self.spacing(1.0)
        for name, base in self.garage.PADDING_TABLE.items():
            with self.subTest(entry=name):
                self.assertIn(f"{base}px", css)

    def test_scale_two_is_the_shipped_table_doubled(self) -> None:
        css = self.spacing(2.0)
        doubled = {base * 2 for base in self.garage.PADDING_TABLE.values()}
        for name, base in self.garage.PADDING_TABLE.items():
            with self.subTest(entry=name):
                self.assertIn(f"{base * 2}px", css)
        # And nothing was left behind at its base value: every pixel the sheet
        # declares is a doubled entry. Two blocks are cut out first because
        # neither is spacing: the dot margin tracks the bar height, and the status
        # tail's width is a measurement of how far that group reaches in from the
        # edge -- see WAYBAR_STATUS_TAIL_WIDTH, which is calibrated at the shipped
        # padding scale and does not follow this slider.
        body = re.sub(r"#workspaces button label \{[^}]*\}", "", css)
        body = re.sub(r"#status-tail \{[^}]*\}", "", body)
        # And the hover pill's margin, which is the dot margin's arithmetic
        # applied to MODULE_HOVER_PILL: the bar height decides it, not this
        # slider. Matched on the declaration rather than the selector list,
        # which is shared with a padding rule that does scale.
        body = re.sub(r"(?:#[\w-]+,\n)+#clock \{\n  margin-top:[^}]*\}", "", body)
        # The menu button's square and the workspace button's vertical margin.
        # Both are shape rather than density -- the square is MODULE_HOVER_PILL
        # wide because that is how tall the hover tint is, and the margin centres
        # the dot's box in the bar. The two margins in the menu block that *are*
        # table entries are checked by the assertIn loop above.
        body = re.sub(r"#custom-menu \{[^}]*\}", "", body)
        body = re.sub(r"#workspaces button \{[^}]*\}", "", body)
        self.assertEqual(set(), {int(value) for value
                                 in re.findall(r"(\d+)px", body)} - doubled,
                         "a pixel value the scale did not reach")

    def test_every_scaled_value_grows_with_the_scale(self) -> None:
        loose = self.pixels(self.spacing(2.0))
        tight = self.pixels(self.spacing(1.0))
        self.assertEqual(len(loose), len(tight))
        for base, scaled in zip(tight, loose):
            self.assertLessEqual(base, scaled)

    def test_the_dot_margin_follows_the_pill_and_not_the_scale(self) -> None:
        """The one value the scale must not reach.

        This margin is the dot's box, and the box is the button's hover tint, so
        it is MODULE_HOVER_PILL tall whatever the density slider says. A doubled
        margin would make one 6px dot wear a taller tint than every module
        beside it.
        """
        pattern = re.compile(r"#workspaces button label \{\s*margin: (\d+)px 0;")
        at_one = pattern.search(self.spacing(1.0, height=43)).group(1)
        at_two = pattern.search(self.spacing(2.0, height=43)).group(1)
        self.assertEqual(at_one, at_two)
        box = int(at_one) * 2 + self.garage.WORKSPACE_DOT
        self.assertLessEqual(abs(box - self.garage.MODULE_HOVER_PILL), 1,
                             "the dot's tint and every other module's are not "
                             "the same height")

    def test_the_dot_is_centred_in_the_bar_by_its_button(self) -> None:
        """What used to be the label's job. The label sizes the tint now, so the
        button's own margin is what puts that tint in the middle of the bar."""
        label = re.compile(r"#workspaces button label \{\s*margin: (\d+)px 0;")
        button = re.compile(r"#workspaces button \{\s*margin: (\d+)px 0;")
        for height in (30, 43, 60):
            with self.subTest(height=height):
                css = self.spacing(1.2, height)
                box = (int(label.search(css).group(1)) * 2
                       + self.garage.WORKSPACE_DOT)
                outer = box + int(button.search(css).group(1)) * 2
                self.assertLessEqual(outer, height,
                                     "the dot's button is taller than the bar")
                self.assertGreaterEqual(outer, height - 1,
                                        "and not so short that it is off-centre")

    def test_the_space_between_two_dots_belongs_to_a_button(self) -> None:
        """The click target, which used to be the 6px dot and nothing else.

        A margin put the gap outside the button, so half of what looks like a
        row of controls answered no clicks at all. As padding it is inside.
        """
        css = self.spacing(1.0)
        button = re.search(r"#workspaces button \{([^}]*)\}", css).group(1)
        self.assertIn(f"padding: 0 {self.garage.PADDING_TABLE['workspace_gap']}px",
                      button)
        self.assertNotRegex(button, r"margin: 0 \d+px",
                            "a horizontal margin here is dead bar again")

    def test_the_workspace_cluster_has_the_same_gap_on_both_sides(self) -> None:
        css = self.spacing(1.2)
        gap = round(self.garage.PADDING_TABLE["menu_right"] * 1.2)
        menu = re.search(r"#custom-menu \{([^}]*)\}", css).group(1)
        media = re.search(r"#custom-media \{([^}]*)\}", css).group(1)
        self.assertIn(f"margin-right: {gap}px", menu)
        self.assertIn(f"margin-left: {gap}px", media)
        self.assertNotIn("padding-left", media,
                         "media padding would paint its hover tint across the gap")

    def test_both_outer_edges_share_the_scaled_gutter(self) -> None:
        css = self.spacing(1.2)
        edge = round(self.garage.PADDING_TABLE["edge"] * 1.2)
        menu = re.search(r"#custom-menu \{([^}]*)\}", css).group(1)
        clock = re.findall(r"#clock \{([^}]*)\}", css)[-1]
        self.assertIn(f"margin-left: {edge}px", menu)
        self.assertIn(f"margin-right: {edge}px", clock)
        self.assertNotIn("padding-right", clock,
                         "clock padding paints its hover tint into the edge gutter")

    def test_the_overrides_come_after_the_import(self) -> None:
        """GTK CSS resolves equal specificity by order, so this is the whole
        mechanism: the base sheet supplies the fonts, this supplies the room."""
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        style = self.garage.waybar_style_css("dark", config)
        self.assertLess(style.index('@import "waybar-base.css"'),
                        style.index("PADDING_TABLE"))

    def test_the_generated_sheet_balances_and_terminates(self) -> None:
        for scale in (1.0, 1.2, 2.0):
            css = self.spacing(scale)
            with self.subTest(scale=scale):
                self.assertEqual(css.count("{"), css.count("}"))
                for line in css.splitlines():
                    if ":" in line and not line.strip().startswith("/*"):
                        self.assertTrue(line.strip().endswith(";"), line)

    def test_the_metric_strips_are_styled_like_the_modules_beside_them(self) -> None:
        # #image is every metric strip and #custom-ai-usage is the text beside
        # them; both have to be reached by the generated spacing or they would sit
        # flush against their neighbours.
        css = self.spacing(1.2)
        self.assertIn("#image {", css)
        self.assertIn("#custom-ai-usage", css)
        # And the base sheet has to give them the bar's colour and font.
        base = BASE_CSS.read_text(encoding="utf-8")
        self.assertIn("#custom-ai-usage", base)


class BarBackground(BackendTestCase):
    """The background enum, as the alpha the bar is drawn with."""

    ALPHA = re.compile(r"@define-color bar_bg rgba\([^)]*?,\s*([0-9.]+)\)")

    def alpha_for(self, scheme: str, background: str) -> float:
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        config["bar"]["background"] = background
        style = self.garage.waybar_style_css(scheme, config)
        return float(self.ALPHA.search(style).group(1))

    def test_transparent_leaves_only_the_blur_layer(self) -> None:
        for scheme in self.garage.SCHEMES:
            with self.subTest(scheme=scheme):
                self.assertEqual(0.0, self.alpha_for(scheme, "transparent"))

    def test_blurred_is_the_palettes_own_tint(self) -> None:
        for scheme in self.garage.SCHEMES:
            with self.subTest(scheme=scheme):
                self.assertEqual(0.42, self.alpha_for(scheme, "blurred"))
                # Read from PALETTE rather than restated here, so the tint stays
                # one number in one place.
                self.assertEqual(self.garage.PALETTE[scheme]["bar_bg"],
                                 self.garage.bar_background(scheme, {
                                     "bar": {"background": "blurred"}}))

    def test_the_body_channels_are_the_same_either_way(self) -> None:
        """Only the alpha moves: the tint direction still follows the appearance,
        so switching to transparent must not change what colour is hiding."""
        for scheme in self.garage.SCHEMES:
            with self.subTest(scheme=scheme):
                blurred = self.garage.bar_background(scheme, {"bar": {"background": "blurred"}})
                clear = self.garage.bar_background(scheme, {"bar": {"background": "transparent"}})
                channels = lambda value: value[:value.rindex(",")]
                self.assertEqual(channels(blurred), channels(clear))

    def test_both_members_of_the_enum_are_renderable(self) -> None:
        for background in self.garage.BAR_BACKGROUNDS:
            for scheme in self.garage.SCHEMES:
                with self.subTest(background=background, scheme=scheme):
                    value = self.garage.bar_background(scheme, {
                        "bar": {"background": background}})
                    self.assertRegex(value, r"\Argba\(\d+, \d+, \d+, [01]\.\d\d\)\Z")


class BarRouting(BackendTestCase):
    """The [bar] section against the schema table's own contract."""

    def entries(self) -> dict:
        return {dotted: entry for dotted, entry
                in self.garage.PREFERENCE_SCHEMA.items()
                if dotted.startswith("bar.")}

    def test_the_section_exists_with_every_key_the_pane_offers(self) -> None:
        expected = {"bar.height", "bar.padding_scale", "bar.background",
                    "bar.ai_usage", "bar.media_player",
                    *(f"bar.monitor_{name}" for name in self.garage.BAR_METRICS)}
        self.assertEqual(expected, set(self.entries()))

    def test_both_bar_routes_exist_and_are_used(self) -> None:
        for route in ("bar_style", "bar_widgets"):
            with self.subTest(route=route):
                self.assertIn(route, self.garage.PREFERENCE_ROUTES)
                self.assertIn(route, {entry.get("route")
                                      for entry in self.entries().values()})

    def test_the_stylesheet_keys_take_the_style_route(self) -> None:
        # background and padding_scale reach waybar/style.css and nothing else.
        for dotted in ("bar.background", "bar.padding_scale"):
            with self.subTest(key=dotted):
                self.assertEqual("bar_style",
                                 self.garage.PREFERENCE_SCHEMA[dotted]["route"])

    def test_the_layout_keys_take_the_widgets_route(self) -> None:
        # height and every toggle reach the fragment, which is a different file
        # with a different writer -- so a key on the wrong route would rewrite the
        # file that does not hold it and leave the one that does untouched.
        for dotted, entry in self.entries().items():
            if dotted in ("bar.background", "bar.padding_scale"):
                continue
            with self.subTest(key=dotted):
                self.assertEqual("bar_widgets", entry["route"])

    def test_neither_route_reaches_the_compositor(self) -> None:
        """The bar is a layer surface with its own config; Hyprland reads neither
        the fragment nor the stylesheet, so no bar change may reload it.

        Read off the step's own source, because that is where the mistake would
        be: `hyprctl reload` costs a visible relayout of every window, and a bar
        route is the last place it belongs.
        """
        for route in ("bar_style", "bar_widgets"):
            for step in self.garage.PREFERENCE_ROUTES[route]:
                name = step if isinstance(step, str) else step[0]
                with self.subTest(route=route, step=name):
                    source = inspect.getsource(getattr(self.garage, name))
                    self.assertNotIn("hyprctl", source)
                    self.assertNotIn("systemctl", source)

    def test_an_unknown_bar_key_is_refused_by_name(self) -> None:
        # bar has no section-wide fallback in SECTION_ROUTES, because every one of
        # its keys routes somewhere of its own -- so an unknown name really is
        # unknown and says so, rather than being applied by whatever the section
        # happens to do.
        self.assertNotIn("bar", self.garage.SECTION_ROUTES)
        with self.assertRaises(self.garage.SettingsError) as raised:
            self.garage.apply_changed_preference({}, "bar.no_such_key")
        self.assertEqual("Unsupported bar preference: no_such_key",
                         str(raised.exception))

    def test_every_bar_key_is_settable(self) -> None:
        for dotted in self.entries():
            with self.subTest(key=dotted):
                config: dict = {}
                self.garage.set_nested(config, dotted, "value")
                self.assertIn("bar", config)

    def test_the_height_range_is_a_bar_rather_than_a_panel(self) -> None:
        entry = self.garage.PREFERENCE_SCHEMA["bar.height"]
        self.assertEqual((30, 60), (entry["minimum"], entry["maximum"]))
        # The strips are drawn 22px tall by the collector; the floor of the
        # range is where the bar still fits one with breathing room.
        self.assertGreaterEqual(entry["minimum"], 22)

    def test_the_scale_starts_at_the_shipped_spacing(self) -> None:
        entry = self.garage.PREFERENCE_SCHEMA["bar.padding_scale"]
        self.assertEqual(1.0, entry["minimum"],
                         "below one the bar would be tighter than PADDING_TABLE, "
                         "which is the spacing the sheet was drawn with")
        self.assertEqual(2.0, entry["maximum"])


class BarRenderers(BackendTestCase):
    """The two renderers write their own file and nothing else."""

    def config(self) -> dict:
        return copy.deepcopy(self.garage.FALLBACK_DEFAULTS)

    def test_render_bar_widgets_writes_only_the_widget_fragment(self) -> None:
        self.garage.render_bar_widgets(self.config())
        written = sorted(path.name for path in self.garage.GENERATED.iterdir())
        self.assertEqual(["waybar-widgets.jsonc"], written)

    def test_applying_widgets_republishes_media_position_before_reloading(self) -> None:
        source = inspect.getsource(self.garage.apply_bar_widgets)
        left = source.index("render_bar_workspaces(config)")
        widgets = source.index("render_bar_widgets(config)")
        reload_position = source.index("reload_bar()")
        self.assertLess(left, widgets)
        self.assertLess(widgets, reload_position)

    def test_render_bar_style_writes_only_the_stylesheet(self) -> None:
        self.garage.render_bar_style(self.config())
        self.assertTrue(self.garage.WAYBAR_STYLE.is_file())
        self.assertFalse(self.garage.GENERATED.exists(),
                         "the stylesheet render reached a generated fragment")

    def test_render_bar_style_keeps_the_inode_waybar_watches(self) -> None:
        # write_marker(), not atomic_write(): a rename past waybar's inotify watch
        # is a change it never hears about.
        config = self.config()
        self.garage.render_bar_style(config)
        before = self.garage.WAYBAR_STYLE.stat().st_ino
        config["bar"]["padding_scale"] = 2.0
        self.garage.render_bar_style(config)
        self.assertEqual(before, self.garage.WAYBAR_STYLE.stat().st_ino)
        self.assertIn(
            f'margin-left: {self.garage.PADDING_TABLE["edge"] * 2}px',
            self.garage.WAYBAR_STYLE.read_text(encoding="utf-8"))

    def test_a_stow_link_at_the_stylesheet_is_replaced_not_written_through(self) -> None:
        # Same migration case as the palette files: ~/.config/waybar/style.css was
        # tracked once, so an unrestowed machine still has a link into the repo.
        tracked = self.root / "tracked-style.css"
        tracked.write_text("the tracked file\n", encoding="utf-8")
        self.garage.WAYBAR_STYLE.parent.mkdir(parents=True, exist_ok=True)
        self.garage.WAYBAR_STYLE.symlink_to(tracked)
        self.garage.render_bar_style(self.config())
        self.assertFalse(self.garage.WAYBAR_STYLE.is_symlink())
        self.assertEqual("the tracked file\n", tracked.read_text(encoding="utf-8"))

    def test_the_render_paths_that_start_the_bar_publish_the_fragment(self) -> None:
        """render_all() and `garage render-bar` both have to write it.

        The second is waybar.service's ExecStartPre and is the reason a missing
        fragment self-heals on boot; the first is `garage render`, which is what
        every other caller uses. A renderer wired into only one of them would
        leave the bar's right side empty on exactly one of the two paths.
        """
        self.assertIn("render_bar_widgets",
                      inspect.getsource(self.garage.render_all))
        main = inspect.getsource(self.garage.main)
        command = main[main.index('elif command == "render-bar"'):
                       main.index('elif command == "render-wallpaper"')]
        self.assertIn("render_bar_widgets(config)", command)

    def test_the_toolkit_render_still_writes_the_bar_stylesheet(self) -> None:
        # render_toolkits() now takes the configuration, and the default has to be
        # the shipped one rather than a crash: a palette render must never be the
        # thing that fails for want of a preference.
        out = self.root / "rendered"
        self.garage.render_toolkits("dark", root=out)
        style = (out / "waybar" / "style.css").read_text(encoding="utf-8")
        self.assertIn("PADDING_TABLE", style)
        self.assertIn('@import "waybar-base.css"', style)


if __name__ == "__main__":
    unittest.main()
