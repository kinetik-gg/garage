"""The palette is one table, and everything that draws with it agrees.

`PALETTE` in the backend is the desktop's whole visual identity: two
appearances, one entry per role, and every toolkit config that carries colour
rendered from it by `render_toolkits()`. Before it there were thirteen hand-kept
copies across six languages, and they had drifted -- the Qt dark palette shared
two colours with the GTK dark palette it was meant to match.

What can still drift, and what each class below pins:

  * The table against itself. A role defined for one appearance and not the
    other is a `KeyError` mid-render, after some files have already been
    written; a composited value in a role Qt reads is a scheme Qt silently
    refuses to load. Both are caught here rather than at the next login.
  * The table against its consumers. Every role a token table names has to
    exist, and every role in the table has to be reached by something --
    otherwise the table grows entries that describe nothing.
  * The rendered files against their parsers. GTK, rasi and qt6ct all fail
    quietly: an unbalanced brace or an `rgba()` where a hex was wanted drops the
    palette and leaves the toolkit's own default, which looks like a theme bug
    rather than a syntax error.
  * The shell against the table. QML cannot @import CSS and cannot read a Python
    dict, so `Theme.qml` and `AppearancePane.qml` keep the only remaining copies
    of the palette and the accent list. They are pinned here instead, which is
    the same trade `test_schema.py` makes for `preferences.defaults.toml`: a
    copy is allowed to exist as long as a test fails when it disagrees.
"""

from __future__ import annotations

from pathlib import Path
import re
import unittest

from harness import BACKEND_PATH, BackendTestCase


REPO_ROOT = Path(__file__).resolve().parent.parent
CONFIG = REPO_ROOT / "desktop" / ".config"
THEME_QML = CONFIG / "quickshell" / "garage" / "Theme.qml"
APPEARANCE_QML = CONFIG / "quickshell" / "garage" / "AppearancePane.qml"

# `#rrggbb`, or the CSS `rgba()` the composited roles are spelled as. Nothing
# else: a three-digit hex, a bare `rgb()` or a named colour would each be legal
# CSS and illegal in at least one of the files the palette is rendered into.
HEX = re.compile(r"#[0-9a-f]{6}\Z")
RGBA = re.compile(r"rgba\((\d{1,3}), (\d{1,3}), (\d{1,3}), (0\.\d\d|1)\)\Z")

# Every colour literal in a QML file, with the two spellings QML accepts:
# `#rrggbb` and the `#aarrggbb` Qt uses for a colour with alpha.
QML_COLOR = re.compile(r'"#((?:[0-9a-fA-F]{2})?)([0-9a-fA-F]{6})"')

# The structural sheets, which the palette is imported *ahead of*. A colour that
# appears in one of these is a colour that cannot follow the appearance: it was
# the notification daemon we replaced whose `.body` rule made notification text
# unreadable on a light desktop, because its base sheet named a dark-only
# literal directly.
STRUCTURAL_SHEETS = (
    CONFIG / "gtk-3.0" / "thunar.css",
    CONFIG / "gtk-3.0" / "pantheon-files.css",
    CONFIG / "waybar" / "waybar-base.css",
    CONFIG / "swayosd" / "swayosd-base.css",
    CONFIG / "rofi" / "apple-base.rasi",
)
STRUCTURAL_COLOR = re.compile(r"#[0-9a-fA-F]{3,8}\b|\brgba?\(\s*\d")

# The palette files render_toolkits() writes, relative to the config root, and
# the appearance each carries. Listed rather than derived so that dropping one
# from the renderer is a failure here and not a silently un-themed toolkit.
PALETTE_FILES = (
    "gtk-3.0/apple-dark.css",
    "gtk-3.0/apple-light.css",
    "gtk-4.0/apple.css",
    "rofi/apple-dark.rasi",
    "rofi/apple-light.rasi",
    "swayosd/swayosd-dark.css",
    "swayosd/swayosd-light.css",
    "qt6ct/colors/vanta.conf",
    "qt6ct/colors/vanta-light.conf",
)

# The palette variables hyprlock.conf dereferences. hyprlang does not error on an
# undefined variable -- it leaves the `$name` in place, the colour parse fails,
# and the lock screen draws with whatever it defaulted to.
HYPRLOCK_VARS = ("outer_color", "inner_color", "font_color", "placeholder_hex",
                 "check_color", "fail_color")


def qml_block(text: str, name: str) -> str:
    """The body of a `name: ({ ... })` QML object literal."""
    start = text.index(f"{name}: ({{") + len(f"{name}: ({{")
    return text[start:text.index("})", start)]


def qml_mapping(text: str, name: str) -> dict[str, str]:
    """A QML object literal of `key: "#colour"` pairs, as a dict."""
    return {key: value.lower() for key, value
            in re.findall(r'(\w+):\s*"(#[0-9a-fA-F]{6,8})"', qml_block(text, name))}


class PaletteTable(BackendTestCase):
    """PALETTE against itself, before anything reads it."""

    def setUp(self) -> None:
        super().setUp()
        self.palette = self.garage.PALETTE

    def test_the_two_appearances_are_the_only_ones(self) -> None:
        # SCHEMES is what every other renderer resolves a theme to, so a third
        # key here would be a palette nothing can select.
        self.assertEqual(sorted(self.palette), sorted(self.garage.SCHEMES))

    def test_every_role_is_defined_for_both_appearances(self) -> None:
        dark, light = self.palette["dark"], self.palette["light"]
        self.assertEqual(
            sorted(dark), sorted(light),
            "a role defined for one appearance only is a KeyError part-way "
            "through a theme switch, with some files already written")

    def test_every_value_is_a_hex_or_an_rgba(self) -> None:
        for scheme, colors in self.palette.items():
            for role, value in colors.items():
                with self.subTest(scheme=scheme, role=role):
                    self.assertTrue(
                        HEX.fullmatch(value) or RGBA.fullmatch(value),
                        f"{value!r} is neither #rrggbb nor rgba(r, g, b, a)")

    def test_rgba_channels_are_bytes(self) -> None:
        for scheme, colors in self.palette.items():
            for role, value in colors.items():
                match = RGBA.fullmatch(value)
                if match is None:
                    continue
                with self.subTest(scheme=scheme, role=role):
                    for channel in match.groups()[:3]:
                        self.assertLessEqual(int(channel), 255, f"{value!r}")

    def test_a_role_is_composited_in_both_appearances_or_neither(self) -> None:
        # Which spelling a role has decides which toolkits may read it, and that
        # cannot depend on the appearance: opaque() is called per scheme, so a
        # role that is a hex in dark and an rgba in light would render one file
        # and raise on the other.
        for role in self.palette["dark"]:
            with self.subTest(role=role):
                self.assertEqual(
                    bool(HEX.fullmatch(self.palette["dark"][role])),
                    bool(HEX.fullmatch(self.palette["light"][role])),
                    "opaque in one appearance and composited in the other")

    def test_every_token_table_names_a_real_role(self) -> None:
        tables = {"GTK3_TOKENS": [role for _, role in self.garage.GTK3_TOKENS],
                  "GTK4_TOKENS": [role for _, role in self.garage.GTK4_TOKENS],
                  "QT_ROLES": [role for entry in self.garage.QT_ROLES
                               for role in entry[1:]]}
        for name, roles in tables.items():
            for role in roles:
                with self.subTest(table=name, role=role):
                    self.assertIn(role, self.palette["dark"])

    def test_every_role_qt_reads_is_opaque(self) -> None:
        # qt6ct's parser takes #rrggbb and nothing else. Handed a composited
        # role it writes the literal text "rgba(...)" into the scheme file, Qt
        # fails to load the whole palette, and every Qt app falls back to
        # Fusion's own grey -- with nothing logged anywhere.
        for entry in self.garage.QT_ROLES:
            for role in entry[1:]:
                for scheme in self.palette:
                    with self.subTest(role=role, scheme=scheme):
                        self.assertTrue(HEX.fullmatch(self.palette[scheme][role]))

    def test_qt_carries_every_colour_role_in_order(self) -> None:
        # Qt's palette is positional: qt6ct writes twenty-two colours per state
        # in QPalette::ColorRole order, and a missing one shifts every role after
        # it onto the wrong colour.
        self.assertEqual(len(self.garage.QT_ROLES), 22)
        self.assertEqual(self.garage.QT_ROLES[0][0], "WindowText")
        self.assertEqual(self.garage.QT_ROLES[-1][0], "Accent")
        names = [entry[0] for entry in self.garage.QT_ROLES]
        self.assertEqual(len(set(names)), len(names))

    def test_every_role_is_read_by_something(self) -> None:
        # A role nothing names is a line that documents nothing, and the next
        # person to edit the palette has no way to tell it apart from one the
        # desktop is actually drawing with.
        #
        # There are two ways to be read. A renderer names the role as a string,
        # which is counted in the source text -- each definition contributes one
        # mention per appearance, so a role that is only ever defined appears
        # exactly twice. Or the shell draws it: Theme.qml cannot name a role, so
        # it carries the colour, and the role is read there if both appearances'
        # values appear in it. ShellPaletteCopies pins that direction as well,
        # so a role reached only this way is still held to the table.
        source = BACKEND_PATH.read_text(encoding="utf-8")
        shell = {match.group(2).lower()
                 for match in QML_COLOR.finditer(THEME_QML.read_text(encoding="utf-8"))}
        for role, value in self.palette["dark"].items():
            drawn_by_shell = (
                HEX.fullmatch(value) and value[1:] in shell
                and self.palette["light"][role][1:] in shell)
            with self.subTest(role=role):
                self.assertTrue(
                    source.count(f'"{role}"') > 2 or drawn_by_shell,
                    f"palette role {role!r} is defined for both appearances and "
                    "then read by neither a renderer nor the shell")

    def test_border_colours_come_from_the_palette(self) -> None:
        # The window border is the one palette role Hyprland reads, in its own
        # rgba(rrggbbaa) spelling. Derived rather than repeated, so a border that
        # no longer matches the desktop cannot be a copy left behind.
        for scheme, colors in self.palette.items():
            active, inactive = self.garage.BORDER_COLORS[scheme]
            self.assertEqual(active, f'rgba({colors["border_active"][1:]}ff)')
            self.assertEqual(inactive, f'rgba({colors["border_inactive"][1:]}ff)')


class AccentTable(BackendTestCase):
    """ACCENTS, and the schema that offers it."""

    def test_every_accent_is_a_plain_hex(self) -> None:
        for name, value in self.garage.ACCENTS.items():
            with self.subTest(accent=name):
                self.assertTrue(HEX.fullmatch(value), value)

    def test_the_schema_offers_exactly_the_accents_that_have_a_colour(self) -> None:
        # An accent the pane can pick but the palette cannot draw falls through
        # to blue in the shell, with nothing said and nothing logged.
        members = self.garage.PREFERENCE_SCHEMA["appearance.accent_color"]["members"]
        self.assertEqual(members, set(self.garage.ACCENTS))

    def test_the_default_accent_is_one_of_them(self) -> None:
        default = self.garage.PREFERENCE_SCHEMA["appearance.accent_color"]["default"]
        self.assertIn(default, self.garage.ACCENTS)

    def test_no_two_accents_are_the_same_colour(self) -> None:
        values = list(self.garage.ACCENTS.values())
        self.assertEqual(len(set(values)), len(values))


class RenderedPaletteFiles(BackendTestCase):
    """The files render_toolkits() writes, against the parsers that read them."""

    def setUp(self) -> None:
        super().setUp()
        self.out = self.root / "rendered"
        for scheme in self.garage.SCHEMES:
            self.garage.render_toolkits(scheme, root=self.out)

    def test_every_palette_file_is_written(self) -> None:
        for relative in PALETTE_FILES:
            with self.subTest(file=relative):
                self.assertTrue((self.out / relative).is_file())

    def test_both_appearances_are_written_whichever_one_is_resolved(self) -> None:
        # Each toolkit's generated entry point imports its palette by name, and
        # GTK treats a missing @import as nothing at all rather than an error.
        # Rendering only the resolved appearance would leave a switch one
        # interrupted render away from a desktop with no palette.
        single = self.root / "one-appearance"
        self.garage.render_toolkits("dark", root=single)
        for relative in PALETTE_FILES:
            with self.subTest(file=relative):
                self.assertTrue((single / relative).is_file())

    def test_a_stale_stow_link_is_replaced_and_not_written_through(self) -> None:
        # The migration case. Until this wave these paths were tracked, so on any
        # machine that has not restowed since, ~/.config still holds a symlink
        # into the checkout. Writing through it would edit the repository from a
        # theme switch -- and silently, because the write succeeds.
        stale = self.root / "tracked-copy.css"
        stale.write_text("the tracked file\n", encoding="utf-8")
        link = self.root / "relinked" / "gtk-3.0" / "apple-dark.css"
        link.parent.mkdir(parents=True, exist_ok=True)
        link.symlink_to(stale)
        self.garage.render_toolkits("dark", root=self.root / "relinked")
        self.assertFalse(link.is_symlink(), "the render wrote through a stow link")
        self.assertEqual(stale.read_text(encoding="utf-8"), "the tracked file\n")
        self.assertIn("@define-color", link.read_text(encoding="utf-8"))

    def test_css_braces_balance(self) -> None:
        for relative in PALETTE_FILES:
            if not relative.endswith((".css", ".rasi")):
                continue
            text = (self.out / relative).read_text(encoding="utf-8")
            with self.subTest(file=relative):
                self.assertEqual(text.count("{"), text.count("}"),
                                 "unbalanced braces; the parser drops the rest "
                                 "of the sheet without saying so")

    def test_css_declarations_are_terminated(self) -> None:
        for relative in PALETTE_FILES:
            if not relative.endswith(".css"):
                continue
            for number, line in enumerate((self.out / relative).read_text(
                    encoding="utf-8").splitlines(), start=1):
                stripped = line.strip()
                if not stripped.startswith(("@define-color", "--")):
                    continue
                with self.subTest(file=relative, line=number):
                    self.assertTrue(stripped.endswith(";"), stripped)

    def test_the_generated_palette_matches_the_table(self) -> None:
        # Spot-checked through the tokens rather than the whole file: what this
        # has to catch is a renderer that stopped reading PALETTE, which shows up
        # on the first token whichever one it is.
        dark = self.garage.PALETTE["dark"]
        gtk3 = (self.out / "gtk-3.0/apple-dark.css").read_text(encoding="utf-8")
        for token, role in self.garage.GTK3_TOKENS:
            with self.subTest(token=token):
                self.assertIn(f"@define-color {token} {dark[role]};", gtk3)

    def test_the_qt_scheme_is_three_rows_of_twenty_two_opaque_colours(self) -> None:
        for relative in ("qt6ct/colors/vanta.conf", "qt6ct/colors/vanta-light.conf"):
            text = (self.out / relative).read_text(encoding="utf-8")
            self.assertIn("[ColorScheme]", text)
            rows = [line for line in text.splitlines() if "_colors=" in line]
            with self.subTest(file=relative):
                self.assertEqual([row.split("=")[0] for row in rows],
                                 ["active_colors", "inactive_colors", "disabled_colors"])
                for row in rows:
                    colors = [part.strip() for part in row.split("=", 1)[1].split(",")]
                    self.assertEqual(len(colors), 22)
                    for color in colors:
                        self.assertTrue(HEX.fullmatch(color), color)

    def test_the_files_that_cannot_spell_alpha_carry_none(self) -> None:
        # kitty, btop, qt6ct and hyprlang all take an opaque colour only.
        for relative in ("qt6ct/colors/vanta.conf", "qt6ct/colors/vanta-light.conf",
                         "kitty/theme.conf", "btop/themes/vanta.theme"):
            with self.subTest(file=relative):
                self.assertNotIn("rgba(", (self.out / relative).read_text(
                    encoding="utf-8"))

    def test_the_lock_screen_gets_every_variable_it_dereferences(self) -> None:
        # hyprlang leaves an undefined $name in place rather than failing, so the
        # colour parse fails instead and the lock screen draws a default.
        theme = (self.garage.GENERATED / "hyprlock-theme.conf").read_text(
            encoding="utf-8")
        conf = (CONFIG / "hypr" / "hyprlock.conf").read_text(encoding="utf-8")
        for name in HYPRLOCK_VARS:
            with self.subTest(variable=name):
                self.assertIn(f"${name} = ", theme)
                self.assertIn(f"${name}", conf.replace(f"${name} = ", ""))

    def test_the_bar_stylesheet_defines_every_colour_the_bar_reads(self) -> None:
        style = (self.out / "waybar/style.css").read_text(encoding="utf-8")
        base = (CONFIG / "waybar" / "waybar-base.css").read_text(encoding="utf-8")
        defined = set(re.findall(r"@define-color (\w+)", style))
        for name in set(re.findall(r"@(\w+)", base)):
            with self.subTest(color=name):
                self.assertIn(name, defined)

    def test_the_osd_stylesheet_defines_every_colour_the_osd_reads(self) -> None:
        style = (self.out / "swayosd/swayosd-dark.css").read_text(encoding="utf-8")
        base = (CONFIG / "swayosd" / "swayosd-base.css").read_text(encoding="utf-8")
        defined = set(re.findall(r"@define-color (\w+)", style))
        for name in set(re.findall(r"@(\w+)", base)) - {"import"}:
            with self.subTest(color=name):
                self.assertIn(name, defined)


class StructuralSheetsCarryNoColour(unittest.TestCase):
    """The sheets the palette is imported ahead of hold no colour of their own."""

    def test_no_literal_colour_in_a_structural_sheet(self) -> None:
        for path in STRUCTURAL_SHEETS:
            found = [line.strip() for line in path.read_text(
                encoding="utf-8").splitlines() if STRUCTURAL_COLOR.search(line)]
            with self.subTest(sheet=path.name):
                self.assertEqual(found, [], (
                    "a literal colour here cannot follow the appearance: it is "
                    "the same rule that left the old notification daemon's body "
                    "text near-white on a light desktop"))


class NoSwayncResidue(unittest.TestCase):
    """swaync is gone: the shell is the notification daemon now.

    A stray reference here is not cosmetic -- it is either a render path still
    writing a file nothing reads, or an action still shelling out to a binary
    the system no longer has, and both fail silently rather than loudly.
    """

    def test_the_backend_names_no_swaync_anything(self) -> None:
        source = BACKEND_PATH.read_text(encoding="utf-8")
        self.assertNotIn("swaync", source)


class ShellPaletteCopies(BackendTestCase):
    """Theme.qml and AppearancePane.qml, which are the last copies.

    QML can neither @import a stylesheet nor read a Python mapping, and the two
    markers the shell does read carry a name rather than a palette. So the shell
    keeps constants and this pins them, rather than teaching the shell to parse
    a generated palette at startup -- which would put the whole appearance
    behind a file read that can fail, on a surface that has no fallback to draw.
    """

    def setUp(self) -> None:
        super().setUp()
        self.theme = THEME_QML.read_text(encoding="utf-8")
        self.appearance = APPEARANCE_QML.read_text(encoding="utf-8")

    def palette_hexes(self) -> set[str]:
        """Every opaque colour in the table, as bare six-digit hex."""
        return {value[1:] for colors in self.garage.PALETTE.values()
                for value in colors.values() if HEX.fullmatch(value)}

    def test_the_shell_accent_table_is_the_backend_accent_table(self) -> None:
        # The shell resolves the accent itself: garage publishes the *name* to
        # the accent marker, and Theme.qml looks the hex up here. A copy that
        # disagrees means the bar and a GTK app draw different blues.
        self.assertEqual(
            qml_mapping(self.theme, "accentPalette"),
            {name: f"#ff{value[1:]}" for name, value in self.garage.ACCENTS.items()},
            "Theme.qml accentPalette has drifted from ACCENTS")

    def test_the_shell_keeps_the_accents_in_the_table_order(self) -> None:
        # The order is the order the Appearance pane draws the swatch row in,
        # which is why ACCENTS is a mapping with a deliberate order rather than a
        # set, and why Theme derives accentNames from accentPalette instead of
        # listing the names again.
        self.assertEqual(list(qml_mapping(self.theme, "accentPalette")),
                         list(self.garage.ACCENTS))
        self.assertIn("accentNames: Object.keys(accentPalette)", self.theme)

    def test_the_appearance_pane_holds_no_accent_list_of_its_own(self) -> None:
        # It had both a name list and a hex table until the palette was
        # consolidated: two copies inside one process, which could offer a swatch
        # the rest of the shell would not draw.
        self.assertNotIn("accentColors", self.appearance)
        self.assertNotIn("accentNames: [", self.appearance)
        self.assertIn("Theme.accentPalette[modelData]", self.appearance)
        self.assertIn("model: Theme.accentNames", self.appearance)

    def test_no_accent_hex_is_written_anywhere_but_the_two_tables(self) -> None:
        # ACCENTS in the backend and accentPalette in Theme.qml are the two
        # copies the toolchain forces; a third would not be pinned by anything.
        accents = {value[1:].lower() for value in self.garage.ACCENTS.values()}
        allowed = {BACKEND_PATH, THEME_QML}
        for path in sorted(CONFIG.rglob("*.qml")) + sorted(CONFIG.rglob("*.css")):
            if path in allowed:
                continue
            found = accents & {match.group(2).lower() for match
                               in QML_COLOR.finditer(path.read_text(encoding="utf-8"))}
            with self.subTest(file=path.name):
                self.assertEqual(found, set(), "an accent hex outside both tables")

    def test_every_colour_in_the_shell_is_a_palette_colour(self) -> None:
        # Alpha is the shell's own: the same body is drawn at several opacities
        # depending on whether the compositor is putting glass behind it. What
        # has to match is the colour underneath.
        accents = qml_block(self.theme, "accentPalette")
        known = self.palette_hexes()
        for match in QML_COLOR.finditer(self.theme):
            if match.start() >= self.theme.index(accents) and \
                    match.end() <= self.theme.index(accents) + len(accents):
                continue
            with self.subTest(literal=match.group(0)):
                self.assertIn(match.group(2).lower(), known,
                              "a colour in Theme.qml that PALETTE does not name")

    def test_the_shell_bodies_are_the_palette_bodies(self) -> None:
        # bodyBase and bodyRaised are what every tinted token in the shell is
        # built from, so these two are the ones a drift would spread from.
        palette = self.garage.PALETTE
        for token, role in (("bodyBase", "body_base"), ("bodyRaised", "bg_float")):
            match = re.search(
                rf'{token}: dark \? "(#[0-9a-fA-F]{{6}})" : "(#[0-9a-fA-F]{{6}})"',
                self.theme)
            with self.subTest(token=token):
                self.assertIsNotNone(match, f"{token} is no longer a plain pair")
                self.assertEqual(match.group(1).lower(), palette["dark"][role])
                self.assertEqual(match.group(2).lower(), palette["light"][role])

    def test_the_shell_corner_power_matches_the_compositor(self) -> None:
        # Not colour, but the same class of copy and the same file: the shell
        # draws Hyprland's superellipse itself and only stays aligned with the
        # glass behind it while these agree.
        match = re.search(r"cornerPower: ([\d.]+)", self.theme)
        self.assertIsNotNone(match)
        self.assertEqual(float(match.group(1)), self.garage.CORNER_POWER)


class GeneratedPaletteIsNotTracked(unittest.TestCase):
    """The rendered palette is ignored by stow and by git, and absent from both."""

    def test_no_tracked_file_sits_where_a_generated_one_is_written(self) -> None:
        # render_toolkits() writes into ~/.config, where every tracked path is a
        # stow symlink into this checkout. A tracked file at one of these paths
        # would be edited in place on every theme switch.
        for relative in PALETTE_FILES:
            with self.subTest(file=relative):
                self.assertFalse((CONFIG / relative).exists(),
                                 "this path is rendered, so it cannot be stowed")

    def test_stow_is_told_to_skip_every_generated_palette_file(self) -> None:
        patterns = [line.strip() for line in (
            REPO_ROOT / "desktop" / ".stow-local-ignore"
        ).read_text(encoding="utf-8").splitlines()
            if line.strip().startswith("^/")]
        for relative in PALETTE_FILES:
            with self.subTest(file=relative):
                self.assertTrue(
                    any(re.search(pattern, f"/.config/{relative}")
                        for pattern in patterns),
                    f"{relative} would be linked into $HOME and then overwritten")

    def test_git_is_told_to_ignore_every_generated_palette_file(self) -> None:
        ignored = set((REPO_ROOT / ".gitignore").read_text(
            encoding="utf-8").split())
        for relative in PALETTE_FILES:
            with self.subTest(file=relative):
                self.assertIn(f"desktop/.config/{relative}", ignored)
