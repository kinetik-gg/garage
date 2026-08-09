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
`protected` is "1" for a bind in binds.lua's RESCUE table and "0" otherwise.

The last line is the witness: "#end \t N", N being the number of rows above it.
Two fields, so the five-field reader skips it as data, and short or missing in
exactly the case the file is a fragment. binds.lua writes the whole thing to
`keybinds.catalog.new` and renames it into place, so a reader now sees one whole
catalog or the previous one -- but the witness is what the reader checks, because
a file written by an older session's binds.lua, or by the fallback direct write
that runs if os.rename is unavailable inside Hyprland's Lua sandbox, can still be
a prefix. `torn_catalog` below reproduces that prefix; a catalog with complete
rows and no witness reproduces the older session.
"""

from __future__ import annotations

import contextlib
import io
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
# Spelled out rather than read off the backend, the same way the five-field
# layout above is: this is the file format the two sides agree on, so a change on
# either side has to come here and be seen.
SENTINEL = "#end"


def catalog_text(rows=CATALOG_ROWS, *, witness: bool = True) -> str:
    """The catalog as binds.lua publishes it: the rows, then the witness line.

    `witness=False` is a catalog written by a binds.lua from before the witness
    existed -- complete, but unprovable.
    """
    lines = ["\t".join(row) for row in rows]
    if witness:
        lines.append(f"{SENTINEL}\t{len(rows)}")
    return "\n".join(lines) + "\n"


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

    def verified(self) -> bool:
        return self.garage.read_keybind_catalog()[1]

    def load_reporting(self) -> tuple[dict, str]:
        """`load_keybindings()` plus whatever it said on stderr.

        Captured rather than left to the terminal both because the note is part
        of the behaviour -- a dropped override has to be reported somewhere the
        journal keeps it -- and so a test that expects a drop does not scribble
        over the suite's own output.
        """
        stream = io.StringIO()
        with contextlib.redirect_stderr(stream):
            document = self.garage.load_keybindings()
        return document, stream.getvalue()

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

    def test_the_witness_line_is_not_read_as_a_bind(self) -> None:
        """It rides in the same file as the binds and must stay invisible to
        everything that lists them."""
        self.assertNotIn(SENTINEL, {entry["id"] for entry in self.catalog()})
        self.assertEqual(len(CATALOG_ROWS), len(self.catalog()))

    def test_the_witness_word_is_the_one_the_backend_looks_for(self) -> None:
        self.assertEqual(SENTINEL, self.garage.KEYBIND_CATALOG_SENTINEL)

    def test_a_whole_catalog_verifies(self) -> None:
        self.assertTrue(self.verified())

    def test_binds_lua_publishes_the_witness_through_a_rename(self) -> None:
        """The producer's half of the format, checked in the Lua source.

        There is no Hyprland here to run binds.lua, so this is what keeps the two
        sides honest: the word this file and the backend agree on has to appear in
        the writer, and the writer has to reach the catalog through a temporary
        and os.rename rather than truncating the file readers are looking at.
        """
        source = BINDS_LUA.read_text(encoding="utf-8")
        publish = source.split("local rows = #catalog", 1)[1]
        self.assertIn(f'"{SENTINEL}\\t"', publish)
        self.assertIn('CATALOG_FILE .. ".new"', publish)
        self.assertIn("pcall(os.rename, temporary, CATALOG_FILE)", publish)


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

    # The key half is normalised the same way the modifiers are, which is what
    # KEY_MODIFIER_ORDER exists for ("so two shortcuts that mean the same thing to
    # the compositor also read the same way in the pane"). It used to pass
    # through verbatim, so a hand-edited 'shift + super + a' was stored and shown
    # as "SUPER + SHIFT + a" beside 'SUPER+SHIFT+A' stored as "SUPER + SHIFT + A".
    def test_the_key_half_is_case_normalised_too(self) -> None:
        canonical = {self.garage.canonical_combination(text) for text in self.SPELLINGS}
        self.assertEqual({"SUPER + SHIFT + A"}, canonical)

    def test_the_whitelists_spelling_is_the_one_adopted(self) -> None:
        """Not str.upper(): the keysym names in KEY_CHOICES are mixed case on
        purpose, and "MINUS" or "PAGE_UP" is not a key."""
        for spelling, canonical in (("SUPER + MINUS", "SUPER + minus"),
                                    ("SUPER + page_up", "SUPER + Page_Up"),
                                    ("SUPER + backspace", "SUPER + BackSpace"),
                                    ("SUPER + rEtUrN", "SUPER + Return")):
            with self.subTest(spelling=spelling):
                self.assertEqual(canonical, self.garage.canonical_combination(spelling))
                self.assertIn(canonical.split(" + ")[-1], self.garage.KEY_CHOICES)

    def test_a_key_outside_the_whitelist_keeps_its_spelling(self) -> None:
        """code:NN, mouse:NNN and the XF86 keysyms are bindable and are not on
        the pane's list, so there is no canonical spelling to adopt: theirs is
        already it, and mangling the case would break the bind."""
        for spelling in ("SUPER + code:82", "SUPER + mouse:273",
                         "XF86AudioRaiseVolume", "SUPER + XF86AudioMute"):
            with self.subTest(spelling=spelling):
                self.assertEqual(spelling.replace("+", " + ").replace("  ", " "),
                                 self.garage.canonical_combination(spelling))

    def test_every_catalog_key_survives_canonicalisation(self) -> None:
        """The keys binds.lua actually writes come back out unchanged in
        meaning, so canonicalising an override cannot make it look like a move
        away from the default it equals."""
        for entry in self.catalog():
            with self.subTest(keys=entry["keys"]):
                canonical = self.garage.canonical_combination(entry["keys"])
                self.assertEqual(self.garage.combination_signature(entry["keys"]),
                                 self.garage.combination_signature(canonical))
                self.assertEqual(entry["id"], self.garage.combination_id(canonical))

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
    """An unprovable catalog must never cost the user their overrides.

    config/binds.lua used to write generated/keybinds.catalog with a plain
    io.open(..., "w") -- truncate, write, close, with no temporary and no
    rename -- so any reader that arrived during a Hyprland reload could see a
    prefix of the file. `load_keybindings()` filtered its overrides against
    whatever ids that prefix contained and `keybind_action()` wrote the filtered
    set straight back over keybindings.toml, so a rebind racing a reload silently
    deleted every other override the user had.

    Both ends are fixed. binds.lua renames a fully written temporary into place,
    and the reader refuses to filter against a catalog it cannot prove is whole:
    the row count on the witness line has to match the rows it parsed. Missing,
    empty, torn, witness-less and miscounted all reach the same non-destructive
    place -- read the catalog, keep every override -- and only a catalog that
    proves itself may drop one.
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
        document, notes = self.load_reporting()
        self.assertEqual(self.OVERRIDES, document["overrides"],
                         "load_keybindings() dropped overrides it could not "
                         "find in an unreadable catalog")
        self.assertEqual("", notes, "nothing was dropped, so nothing to report")
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

    def test_a_torn_catalog_preserves_overrides(self) -> None:
        """The case that used to lose the user's rebinds: a non-empty prefix,
        indistinguishable from a complete short catalog on its rows alone. The
        witness is what distinguishes it -- the tear took it away."""
        baseline = self.write_overrides(self.OVERRIDES)
        self.torn_catalog()
        # Precondition: the tear is the interesting kind -- a non-empty catalog
        # that is missing binds the overrides name.
        catalog = self.catalog()
        self.assertTrue(catalog, "fixture is not a partial catalog")
        self.assertLess(len(catalog), len(CATALOG_ROWS))
        self.assertFalse(self.verified())
        self.assert_overrides_survive(baseline)

    def test_a_witness_less_catalog_preserves_overrides(self) -> None:
        """A session still running the binds.lua that shipped before the witness.

        Complete, but unprovable, and the two cannot be told apart -- so it is
        read and served like any other catalog and simply never filtered against.
        Backward compatibility that costs a stale override, not a working pane.
        """
        baseline = self.write_overrides(self.OVERRIDES)
        self.write_catalog(catalog_text(witness=False))
        self.assertEqual(len(CATALOG_ROWS), len(self.catalog()),
                         "an old catalog must still list every shortcut")
        self.assertFalse(self.verified())
        self.assert_overrides_survive(baseline)

    def test_a_witness_counting_more_rows_than_arrived_preserves_overrides(self) -> None:
        """The tear that ends exactly on a line boundary, and the reason the
        witness carries a count rather than only marking the end: the rows are
        all whole, so nothing but the number gives the loss away."""
        baseline = self.write_overrides(self.OVERRIDES)
        rows = CATALOG_ROWS[:-2]
        self.write_catalog("\n".join("\t".join(row) for row in rows) + "\n"
                           + f"{SENTINEL}\t{len(CATALOG_ROWS)}\n")
        self.assertEqual(len(rows), len(self.catalog()))
        self.assertFalse(self.verified())
        self.assert_overrides_survive(baseline)

    def test_a_witness_counting_fewer_rows_than_arrived_preserves_overrides(self) -> None:
        """The mirror image, so the check is an equality and not a floor."""
        baseline = self.write_overrides(self.OVERRIDES)
        self.write_catalog(catalog_text() + f"{SENTINEL}\t{len(CATALOG_ROWS) - 1}\n")
        self.assertFalse(self.verified())
        self.assert_overrides_survive(baseline)

    def test_a_witness_that_is_not_the_last_line_preserves_overrides(self) -> None:
        """Written last by binds.lua, so anything after it is not a catalog this
        reader wrote and not one it will act on."""
        baseline = self.write_overrides(self.OVERRIDES)
        self.write_catalog(catalog_text() + "super+x\tWindows\tSUPER + X\t0\tLater\n")
        self.assertFalse(self.verified())
        self.assert_overrides_survive(baseline)

    def test_a_verified_catalog_drops_only_the_absent_override(self) -> None:
        """The audit-era intent, kept and confined.

        A shortcut a release really did remove leaves a stale override that would
        fail the guard on every later change, so it is still dropped -- but only
        from a catalog that proved itself whole, only the one id that is missing,
        and never quietly.
        """
        rows = [row for row in CATALOG_ROWS if row[0] != "super+s"]
        self.write_catalog(catalog_text(rows))
        self.assertTrue(self.verified())
        self.write_overrides(self.OVERRIDES)
        document, notes = self.load_reporting()
        self.assertEqual({"super+w": "SUPER + ALT + W",
                          "super+shift+w": "SUPER + ALT + P"}, document["overrides"])
        self.assertIn("super+s", notes)
        self.assertIn("no longer publishes", notes)

    def test_an_unverified_catalog_is_refused_by_name(self) -> None:
        """What replaces the deletion, and it has to be honest.

        An override the fragment has not reached is no longer filtered away, so
        it reaches the guard and stops the change. Telling the user there is no
        such shortcut sends them to edit a file that is already right; the truth
        is that the list is mid-publication.
        """
        # Torn past both rescue rows, so it is the unknown id that stops the
        # change rather than the missing-rescue check ahead of it.
        rows = CATALOG_ROWS[:5]
        self.assertTrue({row[0] for row in rows} >= set(RESCUE_IDS))
        self.write_catalog(catalog_text(rows, witness=False))
        catalog, verified = self.garage.read_keybind_catalog()
        self.assertFalse(verified)
        document = self.document(self.OVERRIDES)
        with self.assertRaises(self.garage.SettingsError) as caught:
            self.garage.guard_keybinds(catalog, document, verified)
        self.assertIn("still being published", str(caught.exception))
        # Verified, the same document is refused with the name of the id, which
        # is the message that means "this override really is stale".
        with self.assertRaises(self.garage.SettingsError) as caught:
            self.garage.guard_keybinds(catalog, document, True)
        self.assertIn("There is no shortcut called", str(caught.exception))

    def test_a_verified_catalog_drops_a_rescue_override_without_a_note(self) -> None:
        """The other half of the filter, unreported on purpose: an override on a
        protected id is a hand edit the guard refuses anyway, so dropping it
        costs nothing the user chose from the pane."""
        self.write_overrides({"super+return": "SUPER + ALT + F12",
                              "super+w": "SUPER + ALT + W"})
        document, notes = self.load_reporting()
        self.assertEqual({"super+w": "SUPER + ALT + W"}, document["overrides"])
        self.assertEqual("", notes)

    def test_a_torn_catalog_is_still_read_as_a_short_but_valid_catalog(self) -> None:
        """The mechanism the bug rested on, pinned rather than removed.

        The entries themselves are unchanged: a fragment still parses as a
        complete shorter catalog and nothing in a row says otherwise. That is
        deliberate -- the rows stay the format the pane draws -- so the whole
        defence is the witness beside them, and this test is what keeps the two
        facts from being confused for each other.
        """
        self.torn_catalog()
        catalog, verified = self.garage.read_keybind_catalog()
        self.assertTrue(catalog)
        self.assertLess(len(catalog), len(CATALOG_ROWS))
        # Nothing in the returned value marks it as incomplete.
        self.assertEqual({"id", "group", "keys", "protected", "description"},
                         set(catalog[0]))
        # Only the second half of the pair does.
        self.assertFalse(verified)

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
