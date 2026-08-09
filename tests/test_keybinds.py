"""Keybind guard properties, exercised through the backend's public functions.

Nothing here reimplements a rule. Every assertion goes through
`load_keybindings()`, `keybind_catalog()`, `resolve_keybinds()`,
`guard_keybinds()`, `keybindings_toml()` and the normalisation helpers, so a
change in how a rule is spelled keeps the test honest and a change in what the
rule *is* breaks it.

The fixture is the real catalog format. config/binds.lua publishes
`generated/keybinds.catalog` as tab-separated lines of five fields --

    id \t group \t keys \t protected \t description

-- where `id` is bind_id(keys), i.e. the default combination with whitespace
removed and lowercased, `keys` is the combination as the Lua file wrote it, and
`protected` is "1" for a bind in binds.lua's RESCUE table and "0" otherwise. The
file is written with a plain io.open(..., "w"), which truncates before it writes:
there is no temporary and no rename, so a reader arriving mid-write sees a
prefix of the lines and possibly a half-written last one. That is what
`torn_catalog` below reproduces.
"""

from __future__ import annotations

import unittest

from harness import BINDS_LUA, BackendTestCase


# Real ids, groups and descriptions, lifted from config/binds.lua. The two
# protected entries are binds.lua's RESCUE table verbatim -- their ids are the
# keys of that table, which is what makes them the rescue binds.
CATALOG_ROWS = [
    ("super+w", "Windows", "SUPER + W", "0", "Close the focused window"),
    ("super+shift+space", "Windows", "SUPER + SHIFT + Space", "0",
     "Toggle the focused window floating"),
    ("super+d", "Windows", "SUPER + D", "0", "Maximise the focused window"),
    ("super+return", "Applications", "SUPER + Return", "1", "Open a terminal"),
    ("super+space", "Applications", "SUPER + Space", "1",
     "Open the application launcher"),
    ("super+shift+w", "Applications", "SUPER + SHIFT + W", "0", "Choose a wallpaper"),
    ("super+s", "Workspaces", "SUPER + S", "0", "Toggle the scratchpad"),
]
RESCUE_IDS = ("super+return", "super+space")


def catalog_text(rows=CATALOG_ROWS) -> str:
    return "\n".join("\t".join(row) for row in rows) + "\n"


class KeybindTestCase(BackendTestCase):
    """A scratch catalog and keybindings.toml, written where the backend looks."""

    def setUp(self) -> None:
        super().setUp()
        self.catalog_path = self.garage.KEYBINDS_CATALOG
        self.keybindings_path = self.garage.KEYBINDINGS_PATH
        self.catalog_path.parent.mkdir(parents=True, exist_ok=True)
        self.keybindings_path.parent.mkdir(parents=True, exist_ok=True)
        self.write_catalog(catalog_text())

    def write_catalog(self, text: str) -> None:
        self.catalog_path.write_text(text, encoding="utf-8")

    def write_overrides(self, overrides: dict[str, str], custom=()) -> bytes:
        """Install a keybindings.toml, in the exact form the backend writes it.

        Generated through `keybindings_toml()` rather than hand-written, so a
        later byte-for-byte comparison is a statement about data surviving and
        not about formatting.
        """
        text = self.garage.keybindings_toml(
            {"overrides": overrides, "custom": list(custom)})
        self.keybindings_path.write_text(text, encoding="utf-8")
        return self.keybindings_path.read_bytes()

    def catalog(self) -> list[dict]:
        return self.garage.keybind_catalog()

    def document(self, overrides: dict[str, str], custom=()) -> dict:
        return {"overrides": dict(overrides), "custom": list(custom)}


class CatalogFormat(KeybindTestCase):
    """The fixture is the format binds.lua actually publishes."""

    def test_catalog_reads_back_as_five_fields(self) -> None:
        catalog = self.catalog()
        self.assertEqual(len(CATALOG_ROWS), len(catalog))
        self.assertEqual(
            {"id", "group", "keys", "protected", "description"}, set(catalog[0]))
        first = catalog[0]
        self.assertEqual("super+w", first["id"])
        self.assertEqual("SUPER + W", first["keys"])
        self.assertFalse(first["protected"])

    def test_ids_are_combination_id_of_their_keys(self) -> None:
        """The Lua side's bind_id() and the Python side's combination_id() agree."""
        for entry in self.catalog():
            self.assertEqual(entry["id"], self.garage.combination_id(entry["keys"]))

    def test_rescue_ids_match_binds_lua(self) -> None:
        """The fixture's protected rows are binds.lua's RESCUE table.

        Read out of the Lua source so that a rescue bind added or removed there
        makes this fixture visibly stale instead of quietly wrong.
        """
        source = BINDS_LUA.read_text(encoding="utf-8")
        table = source.split("local RESCUE = {", 1)[1].split("}", 1)[0]
        declared = tuple(line.split('["', 1)[1].split('"]', 1)[0]
                         for line in table.splitlines() if '["' in line)
        self.assertEqual(sorted(RESCUE_IDS), sorted(declared))
        protected = {entry["id"] for entry in self.catalog() if entry["protected"]}
        self.assertEqual(set(RESCUE_IDS), protected)

    def test_a_line_with_too_few_fields_is_ignored(self) -> None:
        """The reader's own tolerance, which the torn-write tests rely on."""
        self.write_catalog(catalog_text() + "super+x\tWindows\tSUPER + X\n")
        self.assertEqual(len(CATALOG_ROWS), len(self.catalog()))


class CollisionRejection(KeybindTestCase):
    def test_override_onto_a_taken_combination_is_refused(self) -> None:
        """Moving a bind onto a combination another bind already holds."""
        document = self.document({"super+w": "SUPER + D"})
        with self.assertRaises(self.garage.SettingsError) as caught:
            self.garage.guard_keybinds(self.catalog(), document)
        self.assertIn("already used by", str(caught.exception))

    def test_override_onto_a_free_combination_is_allowed(self) -> None:
        """The negative control: the guard is not simply refusing everything."""
        document = self.document({"super+w": "SUPER + CONTROL + F9"})
        self.garage.guard_keybinds(self.catalog(), document)

    def test_two_overrides_onto_one_combination_are_refused(self) -> None:
        document = self.document({"super+w": "SUPER + ALT + K",
                                  "super+d": "SUPER + ALT + K"})
        with self.assertRaises(self.garage.SettingsError):
            self.garage.guard_keybinds(self.catalog(), document)

    def test_a_custom_bind_may_not_take_a_default_combination(self) -> None:
        custom = [self.garage.custom_keybind(
            {"keys": "SUPER + S", "command": "true", "description": "Mine"})]
        with self.assertRaises(self.garage.SettingsError):
            self.garage.guard_keybinds(self.catalog(), self.document({}, custom))

    def test_vacating_a_combination_lets_another_bind_take_it(self) -> None:
        """A swap through a free slot is legal, which is what makes the guard
        a collision check on the *resolved* set rather than on the defaults."""
        document = self.document({"super+w": "SUPER + ALT + W", "super+d": "SUPER + W"})
        self.garage.guard_keybinds(self.catalog(), document)

    def test_the_guard_refuses_an_unpublished_catalog(self) -> None:
        with self.assertRaises(self.garage.SettingsError):
            self.garage.guard_keybinds([], self.document({}))


class RescueProtection(KeybindTestCase):
    """A rescue bind's id cannot be moved and its combination cannot be taken."""

    def test_rebinding_a_rescue_id_is_refused(self) -> None:
        for identifier in RESCUE_IDS:
            with self.subTest(rescue=identifier):
                document = self.document({identifier: "SUPER + ALT + F12"})
                with self.assertRaises(self.garage.SettingsError) as caught:
                    self.garage.guard_keybinds(self.catalog(), document)
                self.assertIn("cannot be changed", str(caught.exception))

    def test_claiming_a_rescue_combination_is_refused(self) -> None:
        for identifier in RESCUE_IDS:
            keys = next(entry["keys"] for entry in self.catalog()
                        if entry["id"] == identifier)
            with self.subTest(rescue=identifier):
                document = self.document({"super+w": keys})
                with self.assertRaises(self.garage.SettingsError):
                    self.garage.guard_keybinds(self.catalog(), document)

    def test_claiming_a_rescue_combination_in_another_spelling_is_refused(self) -> None:
        """Case and spacing are the writer's taste, not an escape hatch."""
        for spelling in ("super+return", "SUPER + RETURN", "  super  +  Return  ",
                         "super+space", "super + SPACE"):
            with self.subTest(spelling=spelling):
                document = self.document({"super+w": spelling})
                with self.assertRaises(self.garage.SettingsError):
                    self.garage.guard_keybinds(self.catalog(), document)

    def test_a_custom_bind_may_not_take_a_rescue_combination(self) -> None:
        custom = [self.garage.custom_keybind(
            {"keys": "super + return", "command": "true", "description": "Mine"})]
        with self.assertRaises(self.garage.SettingsError):
            self.garage.guard_keybinds(self.catalog(), self.document({}, custom))

    def test_rescue_binds_ignore_an_override_on_resolve(self) -> None:
        """Structural, not just guarded: resolve_keybinds never consults an
        override for a protected entry, mirroring bind() in binds.lua."""
        resolved = self.garage.resolve_keybinds(
            self.catalog(), self.document({"super+return": "SUPER + ALT + F12"}))
        terminal = next(entry for entry in resolved if entry["id"] == "super+return")
        self.assertEqual("SUPER + Return", terminal["keys"])
        self.assertFalse(terminal["modified"])

    def test_load_keybindings_drops_an_override_on_a_rescue_id(self) -> None:
        """A hand-edited file naming a rescue bind is dropped on load, so it
        cannot wedge the guard against every later change."""
        self.write_overrides({"super+return": "SUPER + ALT + F12",
                              "super+w": "SUPER + ALT + W"})
        document = self.garage.load_keybindings()
        self.assertNotIn("super+return", document["overrides"])
        self.assertEqual({"super+w": "SUPER + ALT + W"}, document["overrides"])

    def test_a_catalog_with_no_rescue_bind_is_refused(self) -> None:
        rows = [(row[0], row[1], row[2], "0", row[4]) for row in CATALOG_ROWS]
        self.write_catalog(catalog_text(rows))
        with self.assertRaises(self.garage.SettingsError) as caught:
            self.garage.guard_keybinds(self.catalog(), self.document({}))
        self.assertIn("rescue", str(caught.exception))


class Normalisation(KeybindTestCase):
    """'SUPER+SHIFT+A' and 'shift + super + a' are one shortcut."""

    # Every spelling of one shortcut: modifier case, modifier order, whitespace,
    # and the case of the key itself.
    SPELLINGS = ("SUPER+SHIFT+A", "shift + super + a", "  Shift+Super+A  ",
                 "SHIFT+SUPER+a", "super+shift+A")
    # The subset that differs only in the modifiers and the spacing -- the key is
    # written the way the pane writes it, i.e. as it appears in KEY_CHOICES.
    MODIFIER_SPELLINGS = ("SUPER+SHIFT+A", "shift + super + A", "  Shift+Super+A  ",
                          "SHIFT + super  +  A")

    def test_all_spellings_share_one_signature(self) -> None:
        """What the compositor matches on is one value for all five spellings."""
        signatures = {self.garage.combination_signature(text) for text in self.SPELLINGS}
        self.assertEqual(1, len(signatures), signatures)

    def test_modifier_spellings_canonicalise_identically(self) -> None:
        canonical = {self.garage.canonical_combination(text)
                     for text in self.MODIFIER_SPELLINGS}
        self.assertEqual({"SUPER + SHIFT + A"}, canonical)

    # BUG (live, cosmetic): canonical_combination() normalises the modifiers --
    # case and order -- but passes the key half through exactly as written, so
    # 'shift + super + a' is stored and displayed as "SUPER + SHIFT + a" while
    # 'SUPER+SHIFT+A' is stored as "SUPER + SHIFT + A". The two are one shortcut
    # to combination_signature() and to Hyprland, which is why this is display
    # and file contents rather than behaviour: the Keyboard pane shows the same
    # binding two different ways depending on how it was spelled, defeating the
    # stated purpose of KEY_MODIFIER_ORDER ("so two shortcuts that mean the same
    # thing to the compositor also read the same way in the pane").
    #
    # Only reachable through a hand-edited keybindings.toml today: KEY_CHOICES
    # offers letters as "A".."Z", so the pane never sends a lowercase letter. The
    # fix is not str.upper() -- keysym names in KEY_CHOICES are mixed case on
    # purpose ("minus", "Page_Up", "BackSpace") -- it is a case-insensitive
    # lookup of the key against KEY_CHOICES, adopting the whitelist's spelling.
    @unittest.expectedFailure
    def test_the_key_half_is_case_normalised_too(self) -> None:
        canonical = {self.garage.canonical_combination(text) for text in self.SPELLINGS}
        self.assertEqual({"SUPER + SHIFT + A"}, canonical)

    def test_aliases_fold_onto_their_real_modifier(self) -> None:
        """CTRL/META/WIN/MOD4/ALTGR are spellings, not modifiers of their own."""
        self.assertEqual(self.garage.combination_signature("CTRL + A"),
                         self.garage.combination_signature("control+a"))
        self.assertEqual(self.garage.combination_signature("META + A"),
                         self.garage.combination_signature("super+a"))
        self.assertEqual(self.garage.combination_signature("MOD4 + A"),
                         self.garage.combination_signature("SUPER + A"))

    def test_a_repeated_modifier_does_not_change_the_shortcut(self) -> None:
        self.assertEqual(self.garage.combination_signature("SUPER+SUPER+A"),
                         self.garage.combination_signature("super + a"))

    def test_a_collision_between_two_spellings_is_detected(self) -> None:
        """One bind moved to 'SUPER+SHIFT+A', another to 'shift + super + a'."""
        document = self.document({"super+w": "SUPER+SHIFT+A",
                                  "super+d": "shift + super + a"})
        with self.assertRaises(self.garage.SettingsError) as caught:
            self.garage.guard_keybinds(self.catalog(), document)
        self.assertIn("already used by", str(caught.exception))

    def test_a_default_is_collided_with_by_a_reordered_spelling(self) -> None:
        """SUPER + SHIFT + Space is a default; 'space+shift+super' is not a new slot."""
        document = self.document({"super+w": "shift + super + Space"})
        with self.assertRaises(self.garage.SettingsError):
            self.garage.guard_keybinds(self.catalog(), document)

    def test_setting_one_spelling_and_querying_another_resolves_the_same(self) -> None:
        """The id half normalises too: an override written 'SUPER + SHIFT + W'
        is found under the catalog's 'super+shift+w'."""
        self.write_overrides({"  Super+SHIFT + w  ": "shift + super + A"})
        document = self.garage.load_keybindings()
        self.assertEqual({"super+shift+w": "SUPER + SHIFT + A"}, document["overrides"])
        resolved = self.garage.resolve_keybinds(self.catalog(), document)
        wallpaper = next(entry for entry in resolved if entry["id"] == "super+shift+w")
        self.assertEqual("SUPER + SHIFT + A", wallpaper["keys"])
        self.assertTrue(wallpaper["modified"])
        # And the shortcut it now occupies is the one the other spelling names.
        self.assertEqual(self.garage.combination_signature("SUPER+SHIFT+A"),
                         self.garage.combination_signature(wallpaper["keys"]))

    def test_the_id_of_a_combination_is_spelling_independent(self) -> None:
        """`rebind` decides "back to the default" by comparing ids, so the
        spelling a combination arrives in cannot pin a bind that only looks
        changed."""
        self.assertEqual("super+w", self.garage.combination_id("  super + W "))
        self.assertEqual(self.garage.combination_id("SUPER + W"),
                         self.garage.combination_id("super+w"))

    def test_a_standalone_letter_is_refused(self) -> None:
        with self.assertRaises(self.garage.SettingsError):
            self.garage.require_bindable("A")

    def test_a_function_key_may_stand_alone(self) -> None:
        self.assertEqual("F5", self.garage.require_bindable("F5"))


class TornCatalogNonDestruction(KeybindTestCase):
    """An unreadable catalog must never cost the user their overrides.

    config/binds.lua writes generated/keybinds.catalog with a plain
    io.open(..., "w") -- truncate, write, close, with no temporary and no
    rename -- so any reader that arrives during a Hyprland reload can see a
    prefix of the file. `load_keybindings()` filters its overrides against
    whatever ids that prefix contains and `keybind_action()` writes the filtered
    set straight back over keybindings.toml, so a rebind racing a reload can
    silently delete every other override the user had.

    The zero-byte and missing cases are already handled (`if catalog:` guards
    the filter). The partial case is not: a non-empty prefix looks exactly like
    a complete catalog to the reader.
    """

    OVERRIDES = {"super+w": "SUPER + ALT + W", "super+s": "SUPER + ALT + S",
                 "super+shift+w": "SUPER + ALT + P"}

    def torn_catalog(self, fraction: float = 0.4) -> None:
        """The catalog as a reader would see it mid-write: a leading fragment."""
        whole = catalog_text()
        self.write_catalog(whole[:int(len(whole) * fraction)])

    def assert_overrides_survive(self, baseline: bytes) -> None:
        """The load path, then the write-back it feeds, must not lose anything.

        Two assertions in one because they are one property seen twice: the
        document `load_keybindings()` returns is what `keybind_action()` renders
        over keybindings.toml, so a key missing from the first is a key deleted
        from the file.
        """
        document = self.garage.load_keybindings()
        self.assertEqual(self.OVERRIDES, document["overrides"],
                         "load_keybindings() dropped overrides it could not "
                         "find in an unreadable catalog")
        self.garage.atomic_write(self.keybindings_path,
                                 self.garage.keybindings_toml(document))
        self.assertEqual(baseline, self.keybindings_path.read_bytes(),
                         "keybindings.toml was rewritten with overrides missing")

    def test_a_complete_catalog_preserves_every_valid_override(self) -> None:
        """The control: with the whole catalog readable, nothing is dropped."""
        baseline = self.write_overrides(self.OVERRIDES)
        self.assert_overrides_survive(baseline)

    def test_a_missing_catalog_preserves_overrides(self) -> None:
        baseline = self.write_overrides(self.OVERRIDES)
        self.catalog_path.unlink()
        self.assertEqual([], self.catalog())
        self.assert_overrides_survive(baseline)

    def test_an_empty_catalog_preserves_overrides(self) -> None:
        baseline = self.write_overrides(self.OVERRIDES)
        self.write_catalog("")
        self.assertEqual(0, self.catalog_path.stat().st_size)
        self.assertEqual([], self.catalog())
        self.assert_overrides_survive(baseline)

    def test_a_catalog_of_only_a_partial_line_preserves_overrides(self) -> None:
        """Torn so early that no whole line survived: the reader skips the
        fragment, gets an empty catalog, and the `if catalog:` guard holds."""
        baseline = self.write_overrides(self.OVERRIDES)
        self.write_catalog("super+w\tWindows\tSUP")
        self.assertEqual([], self.catalog())
        self.assert_overrides_survive(baseline)

    # BUG (live, Wave 4.2 to fix): keybind_catalog() cannot tell a truncated
    # catalog from a complete one, so load_keybindings() treats every override
    # whose bind has not been written yet as naming a shortcut that does not
    # exist and drops it. keybind_action() then writes that filtered set over
    # keybindings.toml -- a permanent, silent loss of the user's rebinds,
    # triggered by nothing more than changing one shortcut while Hyprland is
    # reloading. Flip this to a plain test once the reader refuses to filter
    # against a catalog it cannot prove is whole.
    @unittest.expectedFailure
    def test_a_torn_catalog_preserves_overrides(self) -> None:
        baseline = self.write_overrides(self.OVERRIDES)
        self.torn_catalog()
        # Precondition: the tear is the interesting kind -- a non-empty catalog
        # that is missing binds the overrides name.
        catalog = self.catalog()
        self.assertTrue(catalog, "fixture is not a partial catalog")
        self.assertLess(len(catalog), len(CATALOG_ROWS))
        self.assert_overrides_survive(baseline)

    def test_a_torn_catalog_is_read_as_a_short_but_valid_catalog(self) -> None:
        """Documents the mechanism the bug rests on, so the finding survives
        even if the expectedFailure above is flipped by a different fix."""
        self.torn_catalog()
        catalog = self.catalog()
        self.assertTrue(catalog)
        self.assertLess(len(catalog), len(CATALOG_ROWS))
        # Nothing in the returned value marks it as incomplete.
        self.assertEqual({"id", "group", "keys", "protected", "description"},
                         set(catalog[0]))

    def test_custom_binds_survive_a_torn_catalog(self) -> None:
        """Custom shortcuts are never filtered against the catalog, so they are
        the half of the file that is already safe. Pinned so a fix for the
        overrides does not regress them."""
        custom = [self.garage.custom_keybind(
            {"keys": "SUPER + ALT + T", "command": "kitty", "description": "Mine"},
            "abc123abc123")]
        self.write_overrides({}, custom)
        self.torn_catalog()
        document = self.garage.load_keybindings()
        self.assertEqual(custom, document["custom"])


if __name__ == "__main__":
    unittest.main()
