"""Structural regressions for the two-surface launcher.

The launcher changes its visible height as a query gains and loses results.  Its
interactive layer must not resize with that panel: that was the last changing
geometry shared by the field and delegates.  Kinetik Glass, meanwhile, paints a
layer's complete box regardless of client alpha, so the fixed transparent area
cannot itself be the glass layer.  These tests pin that split at both ends.
"""

from __future__ import annotations

import json
from pathlib import Path
import re
import shutil
import subprocess
import unittest

try:
    from harness import BackendTestCase
except ModuleNotFoundError:  # `python -m unittest tests.test_launcher`
    from tests.harness import BackendTestCase


REPO_ROOT = Path(__file__).resolve().parent.parent
CONFIG = REPO_ROOT / "desktop" / ".config"
LAUNCHER = CONFIG / "quickshell" / "garage" / "LauncherPalette.qml"
LAUNCHER_EXTRAS = CONFIG / "quickshell" / "garage" / "LauncherExtras.js"
LAUNCHER_SOURCES = CONFIG / "quickshell" / "garage" / "LauncherSources.qml"
TIMER_SERVICE = CONFIG / "quickshell" / "garage" / "TimerService.qml"
SESSION = CONFIG / "quickshell" / "garage" / "SessionPalette.qml"
SHELL = CONFIG / "quickshell" / "garage" / "shell.qml"
DECORATIONS = CONFIG / "hypr" / "config" / "decorations.lua"
WINDOW_RULES = CONFIG / "hypr" / "config" / "windowrules.lua"


class StableLauncherSurface(unittest.TestCase):
    def setUp(self) -> None:
        self.qml = LAUNCHER.read_text(encoding="utf-8")

    def test_interactive_surface_height_has_no_result_dependency(self) -> None:
        start = self.qml.index("readonly property real surfaceHeight:")
        end = self.qml.index("readonly property real bodyHeight:", start)
        definition = self.qml[start:end]

        self.assertIn("launcher.maxRows * launcher.rowHeight", definition)
        for volatile in ("rowCount", "results", "body.implicitHeight"):
            with self.subTest(volatile=volatile):
                self.assertNotIn(volatile, definition)
        self.assertIn("implicitHeight: launcher.surfaceHeight", self.qml)

    def test_only_visible_geometry_tracks_content_height(self) -> None:
        self.assertIn("implicitHeight: launcher.contentHeight", self.qml)
        self.assertIn("height: launcher.contentHeight", self.qml)
        self.assertIn("mask: Region { item: panel }", self.qml)
        self.assertIn("mask: Region {}", self.qml)

    def test_glass_backing_cannot_take_input(self) -> None:
        start = self.qml.index("id: glassSurface")
        end = self.qml.index("// The search engine", start)
        glass = self.qml[start:end]

        self.assertIn("focusable: false", glass)
        self.assertIn("WlrLayershell.layer: WlrLayer.Top", glass)
        self.assertIn(
            "WlrLayershell.keyboardFocus: WlrKeyboardFocus.None", glass)

    def test_query_results_are_debounced_and_committed_after_delegate_creation(self) -> None:
        self.assertIn("id: rebuildTimer", self.qml)
        self.assertIn("interval: 55", self.qml)
        self.assertIn("id: geometryCommit", self.qml)
        self.assertIn("resultList.itemAtIndex(last) === null", self.qml)
        self.assertLess(
            self.qml.index("launcher.pendingRowCount = displayedRows.length"),
            self.qml.index("launcher.rowCount = launcher.pendingRowCount"))

    def test_file_results_commit_only_after_the_matching_query_finishes(self) -> None:
        sources = LAUNCHER_SOURCES.read_text(encoding="utf-8")
        self.assertIn("if (extraSources.filePending(launcher.query))", self.qml)
        self.assertNotIn("fileWaits", self.qml)
        self.assertNotIn("sources.fileResults = [];", sources)

    def test_filtering_rewrites_a_preallocated_model_without_changing_its_size(self) -> None:
        start = self.qml.index("function rebuild()")
        end = self.qml.index("ListModel { id: results }", start)
        rebuild = self.qml[start:end]

        self.assertIn("results.set(index, row)", rebuild)
        for mutation in ("results.clear", "results.append", "results.remove"):
            with self.subTest(mutation=mutation):
                self.assertNotIn(mutation, rebuild)
        self.assertIn("opacity: launcher.listing ? 1 : 0", self.qml)
        self.assertNotIn("resultsReady", self.qml)

    def test_field_has_an_explicit_origin_outside_vertical_layout(self) -> None:
        body_start = self.qml.index("id: body")
        list_end = self.qml.index("delegate: LauncherResult", body_start)
        body = self.qml[body_start:list_end]

        self.assertIn("anchors.top: parent.top", body)
        self.assertIn("id: fieldRow", body)
        self.assertIn("height: launcher.fieldHeight", body)
        self.assertIn("anchors.top: fieldRow.bottom", body)
        self.assertNotIn("ColumnLayout {", body)

    def test_application_rank_helper_is_imported_before_use(self) -> None:
        self.assertIn('import "LauncherExtras.js" as LauncherExtras', self.qml)
        self.assertIn("LauncherExtras.applicationRank(entry, needle)", self.qml)


class LauncherGlassRouting(unittest.TestCase):
    def test_fixed_host_is_not_a_liquid_glass_surface(self) -> None:
        decorations = DECORATIONS.read_text(encoding="utf-8")
        line = next(line for line in decorations.splitlines()
                    if "layer_namespaces =" in line)

        self.assertIn("garage-launcher-glass", line)
        self.assertNotIn("garage-launcher-host", line)

    def test_legacy_surface_keeps_glass_during_a_staggered_reload(self) -> None:
        decorations = DECORATIONS.read_text(encoding="utf-8")
        line = next(line for line in decorations.splitlines()
                    if "layer_namespaces =" in line)

        self.assertIn("garage-launcher,", line)

    def test_backing_gets_blur_and_both_surfaces_disable_compositor_motion(self) -> None:
        rules = WINDOW_RULES.read_text(encoding="utf-8")
        blur_start = rules.index('name = "apple-dark-shell-blur"')
        blur_end = rules.index("})", blur_start)
        blur_rule = rules[blur_start:blur_end]
        blur_match = next(line for line in blur_rule.splitlines()
                          if "match =" in line)
        motion_start = rules.index('name = "static-shell-layers"')
        motion_end = rules.index("})", motion_start)
        motion_rule = rules[motion_start:motion_end]

        self.assertIn("garage-launcher-glass", blur_rule)
        self.assertNotIn("garage-launcher-host", blur_match)
        self.assertIn("|garage-launcher|", motion_rule)
        self.assertIn("|garage-launcher-host|", motion_rule)
        self.assertIn("|garage-launcher-glass|", motion_rule)


@unittest.skipUnless(shutil.which("node"), "Node is required to execute QML JavaScript parsers")
class LauncherExtraQueries(unittest.TestCase):
    def evaluate(self, expression: str):
        script = f"""
const fs = require("fs");
const vm = require("vm");
const context = {{}};
vm.createContext(context);
const source = fs.readFileSync({json.dumps(str(LAUNCHER_EXTRAS))}, "utf8")
    .replace(/^\\.pragma library\\s*/, "");
vm.runInContext(source, context, {{ filename: "LauncherExtras.js" }});
process.stdout.write(JSON.stringify({expression}));
"""
        result = subprocess.run(
            [shutil.which("node"), "-e", script], check=True,
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return json.loads(result.stdout)

    def test_unit_conversion_is_regex_parsed_across_unit_families(self) -> None:
        rows = self.evaluate('[context.unitConversion("1m in inch"), '
                             'context.unitConversion("100 C to F"), '
                             'context.unitConversion("1 kg in lb"), '
                             'context.unitConversion("1 kg in litre")]')
        self.assertEqual("1 m = 39.3700787402 in", rows[0]["title"])
        self.assertEqual("100 °C = 212 °F", rows[1]["title"])
        self.assertEqual("2.2046226218 lb", rows[2]["title"].split(" = ")[1])
        self.assertIsNone(rows[3])

    def test_currency_parser_accepts_symbols_codes_and_names_only(self) -> None:
        rows = self.evaluate('(() => { const one = context.currencyRequest("$1 to IDR"); '
                             'return [one, context.currencyRequest("1,000 USD in Rupiah"), '
                             'context.currencyRequest("$1 to IDR; reboot"), '
                             'context.currencyResult(one, 17809, "2026-08-11")]; })()')
        self.assertEqual(
            {"amount": 1, "base": "USD", "quote": "IDR", "pair": "USD/IDR"},
            rows[0])
        self.assertEqual(1000, rows[1]["amount"])
        self.assertEqual("USD/IDR", rows[1]["pair"])
        self.assertIsNone(rows[2])
        self.assertEqual("1 USD = 17,809 IDR", rows[3]["title"])

    def test_emoji_search_returns_copyable_keyword_matches(self) -> None:
        rows = self.evaluate('context.emojiRows("emoji love", 8)')
        self.assertGreater(len(rows), 1)
        self.assertLessEqual(len(rows), 8)
        self.assertTrue(all(row["kind"] == "emoji" for row in rows))
        self.assertTrue(all("love" in row["subtitle"] for row in rows))
        self.assertTrue(all(row["value"] in row["title"] for row in rows))

    def test_application_ranking_prefers_graphical_desktop_apps(self) -> None:
        ranks = self.evaluate('({ desktop: context.applicationRank('
                              '{ name: "Quick GUI", genericName: "", comment: "tool", '
                              'runInTerminal: false, noDisplay: false }, "quick"), '
                              'desktopComment: context.applicationRank('
                              '{ name: "Workbench", genericName: "", comment: "quick tool", '
                              'runInTerminal: false, noDisplay: false }, "quick"), '
                              'cliExact: context.applicationRank('
                              '{ name: "Quick CLI", genericName: "", comment: "", '
                              'runInTerminal: true, noDisplay: false }, "quick"), '
                              'hidden: context.applicationRank('
                              '{ name: "Quick Hidden", genericName: "", comment: "", '
                              'runInTerminal: false, noDisplay: true }, "quick") })')
        self.assertLess(ranks["desktop"], ranks["cliExact"])
        self.assertLess(ranks["desktopComment"], ranks["cliExact"])
        self.assertEqual(-1, ranks["hidden"])

    def test_power_media_and_shell_commands_are_explicit_rows(self) -> None:
        result = self.evaluate('({ power: context.powerRows("power"), '
                               'media: context.mediaRows("audio"), '
                               'dnd: context.shellRows("dnd", true, false, true), '
                               'theme: context.shellRows("light", false, false, true) })')
        self.assertEqual(
            {"poweroff", "restart", "suspend", "logout", "lock"},
            {row["action"] for row in result["power"]})
        self.assertEqual(
            {"play", "pause", "stop", "skip", "mute"},
            {row["action"] for row in result["media"]})
        self.assertEqual("Turn Off Do Not Disturb", result["dnd"][0]["title"])
        self.assertEqual("Switch to Light Appearance", result["theme"][0]["title"])

    def test_uuid_and_rand_results_are_well_formed_and_bounded(self) -> None:
        result = self.evaluate('({ uuid: context.uuidV4(), digits: context.randomDigits(24), '
                               'tooLong: context.utilitySpec("rand(129)") })')
        self.assertRegex(
            result["uuid"],
            r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        self.assertRegex(result["digits"], r"^[0-9]{24}$")
        self.assertEqual("error", result["tooLong"]["kind"])

    def test_timer_stopwatch_and_file_queries_are_unambiguous(self) -> None:
        result = self.evaluate('({ timer: context.timerSpec("timer 1h 30m Tea"), '
                               'tooLong: context.timerSpec("timer 8d"), '
                               'stopwatch: context.stopwatchSpec("stopwatch lap"), '
                               'file: context.fileSearchQuery("file launch notes"), '
                               'plain: context.fileSearchQuery("firefox") })')
        self.assertEqual("start", result["timer"]["mode"])
        self.assertEqual(5_400_000, result["timer"]["durationMs"])
        self.assertEqual("Tea", result["timer"]["label"])
        self.assertEqual("error", result["tooLong"]["mode"])
        self.assertEqual("lap", result["stopwatch"]["action"])
        self.assertEqual("launch notes", result["file"])
        self.assertIsNone(result["plain"])

    def test_pid_search_is_fuzzy_and_ssh_rejects_shell_syntax(self) -> None:
        result = self.evaluate('({ query: context.killQuery("kill qsh"), '
                               'processes: context.processRows("qsh", '
                               'context.parseProcessList("42 quickshell /usr/bin/quickshell -c garage\\n77 bash bash"), 8), '
                               'ssh: context.sshSpec("ssh rizki@example.com"), '
                               'unsafe: context.sshSpec("ssh host; reboot") })')
        self.assertEqual("qsh", result["query"])
        self.assertEqual(42, result["processes"][0]["pid"])
        self.assertEqual("rizki@example.com", result["ssh"]["target"])
        self.assertEqual("error", result["unsafe"]["kind"])


class LauncherActionRouting(unittest.TestCase):
    def test_power_actions_open_the_existing_session_confirmation(self) -> None:
        launcher = LAUNCHER.read_text(encoding="utf-8")
        shell = SHELL.read_text(encoding="utf-8")
        session = SESSION.read_text(encoding="utf-8")

        self.assertIn("signal sessionActionRequested(string action)", launcher)
        self.assertIn('shell.openSurface("session"', shell)
        self.assertIn("initialAction: shell.sessionInitialAction", shell)
        self.assertIn("property string pendingAction: menu.initialAction", session)
        self.assertIn('"lock": "Lock this system?"', session)

    def test_pid_and_ssh_actions_keep_user_input_out_of_a_shell(self) -> None:
        launcher = LAUNCHER.read_text(encoding="utf-8")
        self.assertIn('["kill", "-TERM", String(row.pid)]', launcher)
        self.assertIn('["uwsm", "app", "-T", "ssh", "--", row.target]', launcher)
        self.assertNotRegex(launcher, r'sh",\s*"-c"[^\n]*(?:row\.pid|row\.target)')

    def test_external_data_lands_before_the_stable_model_is_rewritten(self) -> None:
        sources = LAUNCHER_SOURCES.read_text(encoding="utf-8")
        self.assertIn("signal changed()", sources)
        self.assertIn("sources.changed();", sources)
        self.assertIn("onChanged: launcher.scheduleRebuild()", LAUNCHER.read_text(encoding="utf-8"))

    def test_timer_state_outlives_the_launcher_and_exact_actions_come_first(self) -> None:
        timer = TIMER_SERVICE.read_text(encoding="utf-8")
        shell = SHELL.read_text(encoding="utf-8")
        self.assertIn("readonly property var timerService: TimerService", shell)
        self.assertIn('clock-state.json', timer)
        self.assertIn("atomicWrites: true", timer)
        self.assertIn("[control].concat(rows)", timer)


class LauncherBackendActions(BackendTestCase):
    def test_night_shift_toggle_uses_the_normal_locked_preference_route(self) -> None:
        config = self.garage.shipped_defaults()
        original = bool(config["appearance"]["night_shift_enabled"])
        saved = []
        applied = []
        self.garage.load_preferences = lambda: config
        self.garage.save_preferences = lambda value: saved.append(value)
        self.garage.apply_changed_preference = (
            lambda value, dotted: applied.append((value, dotted)))

        self.garage.action("appearance.night_shift.toggle", None)

        self.assertEqual(not original, config["appearance"]["night_shift_enabled"])
        self.assertEqual([config], saved)
        self.assertEqual([(config, "appearance.night_shift_enabled")], applied)


if __name__ == "__main__":
    unittest.main()
