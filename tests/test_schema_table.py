"""The schema table validates itself.

PREFERENCE_SCHEMA is now the only Python-side place a preference is declared:
FALLBACK_DEFAULTS, the keys `set` accepts, the whole of validate_preferences()
and apply_changed_preference()'s routing are all derived from it. That is the
point of the table -- five surfaces that had to be kept in step by hand became
one -- but it moves the failure mode rather than removing it. A table entry can
still be wrong on its own terms:

  * a kind nothing knows how to check, which would raise KeyError on the load
    path and take every render down with it;
  * an "enum" with no members, or an "int" with no range, which would raise the
    same way;
  * a misspelled constraint -- "minumum" -- which would be silently ignored and
    leave the key range-checked against nothing;
  * a route naming a function that does not exist, or one that does not take the
    arguments named beside it: the exact bug class the table was built to end,
    because it fires only when a user changes that one setting;
  * no route at all, which is a key that saves and then silently never reaches
    the running desktop;
  * a key here with no line in preferences.defaults.toml, or the reverse.

So everything the table asserts about itself is asserted back here. These are
meta-tests: they read the table rather than exercising behaviour, which is what
lets them cover a key that does not exist yet. test_schema.py stays the
values-level check against the shipped file; test_preferences.py stays the
behaviour.
"""

from __future__ import annotations

import copy
import inspect
from pathlib import Path
import tomllib
import unittest

from harness import DEFAULTS_TOML, BackendTestCase


# Fields any entry may carry whatever its kind. Anything else has to be declared
# by the kind, through PREFERENCE_KIND_CONSTRAINTS, or it is a typo.
OPTIONAL_FIELDS = {"route", "renames", "also"}
REQUIRED_FIELDS = {"default", "kind"}


def flat_toml_keys(path: Path) -> set[str]:
    """Every "section.key" in the shipped defaults file."""
    with path.open("rb") as handle:
        shipped = tomllib.load(handle)
    return {f"{section}.{key}"
            for section, values in shipped.items()
            if isinstance(values, dict)
            for key in values}


class TableShape(BackendTestCase):
    """Every entry is well formed on its own terms."""

    def entries(self):
        return self.garage.PREFERENCE_SCHEMA.items()

    def test_every_key_is_section_dot_name(self) -> None:
        """Two levels, exactly. set_nested() splits on the dot and refuses three."""
        for dotted in self.garage.PREFERENCE_SCHEMA:
            with self.subTest(key=dotted):
                self.assertEqual(2, len(dotted.split(".")),
                                 "a schema key is 'section.name' and nothing else")
                section, name = dotted.split(".")
                self.assertTrue(section and name, "neither half may be empty")

    def test_every_entry_declares_a_kind_the_backend_knows(self) -> None:
        """An unknown kind is a KeyError inside validate_preferences()."""
        for dotted, entry in self.entries():
            with self.subTest(key=dotted):
                self.assertIn("kind", entry)
                self.assertIn(entry["kind"], self.garage.PREFERENCE_KINDS)

    def test_every_entry_declares_a_default(self) -> None:
        for dotted, entry in self.entries():
            with self.subTest(key=dotted):
                self.assertIn("default", entry,
                              "the default is what a bad value is coerced to")

    def test_every_kind_has_a_constraint_contract(self) -> None:
        """A kind is either constraint-free or says which constraints it needs.

        Otherwise a new kind could be added with a range it reads and nothing
        would check that the entries using it actually declare one.
        """
        for kind in self.garage.PREFERENCE_KINDS:
            with self.subTest(kind=kind):
                required = self.garage.PREFERENCE_KIND_CONSTRAINTS.get(kind, ())
                self.assertIsInstance(required, tuple)

    def test_constraints_are_exactly_what_the_kind_requires(self) -> None:
        """No missing range, no leftover one, and no misspelled field.

        The negative half matters as much as the positive: `members` on an "int"
        is a constraint nothing reads, which usually means the kind is wrong.
        """
        for dotted, entry in self.entries():
            required = set(self.garage.PREFERENCE_KIND_CONSTRAINTS.get(entry["kind"], ()))
            allowed = REQUIRED_FIELDS | OPTIONAL_FIELDS | required
            with self.subTest(key=dotted):
                self.assertEqual(set(), required - set(entry),
                                 f"{entry['kind']} needs these and they are missing")
                self.assertEqual(set(), set(entry) - allowed,
                                 "field the kind does not use -- a typo, or the "
                                 "wrong kind")

    def test_numeric_ranges_are_finite_and_ordered(self) -> None:
        for dotted, entry in self.entries():
            if entry["kind"] not in {"int", "float"}:
                continue
            with self.subTest(key=dotted):
                low, high = entry["minimum"], entry["maximum"]
                for bound in (low, high):
                    self.assertIsInstance(bound, (int, float))
                    self.assertNotIsInstance(bound, bool)
                self.assertLessEqual(low, high, "an empty range coerces every value")

    def test_enums_offer_something_to_choose(self) -> None:
        for dotted, entry in self.entries():
            if entry["kind"] != "enum":
                continue
            with self.subTest(key=dotted):
                members = entry["members"]
                self.assertTrue(len(members), "an empty enum coerces every value")
                for member in members:
                    self.assertIsInstance(member, str,
                                          "the pane sends JSON strings for a choice")

    def test_renames_map_withdrawn_strings_onto_current_values(self) -> None:
        """A rename has to land somewhere the check will then accept.

        A rename onto a value the kind still refuses would coerce twice: once
        silently, once with a note, and the note would name a value the user
        never wrote.
        """
        for dotted, entry in self.entries():
            renames = entry.get("renames")
            if not renames:
                continue
            check = self.garage.PREFERENCE_KINDS[entry["kind"]]
            for withdrawn, current in renames.items():
                with self.subTest(key=dotted, withdrawn=withdrawn):
                    self.assertIsInstance(withdrawn, str)
                    self.assertTrue(check(entry, current),
                                    f"{withdrawn!r} -> {current!r} is still not valid")

    def test_hooks_are_callable_with_the_arguments_they_are_given(self) -> None:
        for dotted, entry in self.entries():
            with self.subTest(key=dotted):
                if "also" in entry:
                    self.assertEqual(2, len(inspect.signature(entry["also"]).parameters),
                                     "`also` is called with (config, value)")
                if "normalise" in entry:
                    self.assertEqual(1, len(inspect.signature(entry["normalise"]).parameters),
                                     "`normalise` is called with the stored value")


class TableAgainstItsOwnDefaults(BackendTestCase):
    """The shipped defaults have to survive the constraints they ship with."""

    def test_every_default_satisfies_its_own_kind(self) -> None:
        for dotted, entry in self.garage.PREFERENCE_SCHEMA.items():
            if entry["kind"] in {"bookkeeping", "normalised"}:
                continue
            check = self.garage.PREFERENCE_KINDS[entry["kind"]]
            with self.subTest(key=dotted):
                self.assertTrue(check(entry, entry["default"]),
                                f"{entry['default']!r} is not a valid "
                                f"{entry['kind']} -- a default the schema itself "
                                "would coerce")

    def test_a_normalised_default_is_already_normal(self) -> None:
        """Otherwise the first load of a fresh install would rewrite the file."""
        for dotted, entry in self.garage.PREFERENCE_SCHEMA.items():
            if entry["kind"] != "normalised":
                continue
            with self.subTest(key=dotted):
                self.assertEqual(entry["default"], entry["normalise"](entry["default"]))

    def test_validating_the_shipped_defaults_says_nothing(self) -> None:
        """The whole property, end to end, through the real pass.

        One assertion covering every entry: if any shipped default fell outside
        its own constraint, a fresh install would report a coercion at every
        session start and the file would disagree with the code that ships it.
        """
        notes: list[str] = []
        self.garage.validate_preferences(
            copy.deepcopy(self.garage.FALLBACK_DEFAULTS), notes)
        self.assertEqual([], notes)


class DerivedSurfaces(BackendTestCase):
    """What used to be hand-maintained is now a function of the table."""

    def test_fallback_defaults_is_the_tables_defaults(self) -> None:
        self.assertEqual(self.garage.schema_defaults(), self.garage.FALLBACK_DEFAULTS)

    def test_fallback_defaults_is_two_levels_of_mapping(self) -> None:
        """The shape every consumer expects, and the shape the TOML has."""
        for section, values in self.garage.FALLBACK_DEFAULTS.items():
            with self.subTest(section=section):
                self.assertIsInstance(values, dict)
                for key, value in values.items():
                    self.assertNotIsInstance(value, (dict, list),
                                             f"{section}.{key} is not a leaf")

    def test_every_table_key_reaches_fallback_defaults(self) -> None:
        flattened = {f"{section}.{key}"
                     for section, values in self.garage.FALLBACK_DEFAULTS.items()
                     for key in values}
        self.assertEqual(set(self.garage.PREFERENCE_SCHEMA), flattened)

    def test_the_table_and_the_shipped_file_have_the_same_keys(self) -> None:
        """Asserted against the table directly, not only through the derivation.

        test_schema.py compares FALLBACK_DEFAULTS with the file; this says the
        same thing one step earlier, so a failure points at the entry rather than
        at the dict it was folded into.
        """
        shipped = flat_toml_keys(DEFAULTS_TOML)
        self.assertEqual(set(), set(self.garage.PREFERENCE_SCHEMA) - shipped,
                         "in PREFERENCE_SCHEMA with no line in "
                         "preferences.defaults.toml")
        self.assertEqual(set(), shipped - set(self.garage.PREFERENCE_SCHEMA),
                         "shipped in preferences.defaults.toml with no entry in "
                         "PREFERENCE_SCHEMA")

    def test_validated_sections_are_every_section_but_the_stamp(self) -> None:
        self.assertEqual(self.garage.schema_sections(), self.garage.VALIDATED_SECTIONS)
        self.assertNotIn("schema", self.garage.VALIDATED_SECTIONS)
        self.assertEqual(set(self.garage.FALLBACK_DEFAULTS) - {"schema"},
                         set(self.garage.VALIDATED_SECTIONS))

    def test_set_accepts_exactly_the_tables_keys(self) -> None:
        """set_nested() gates on the derived defaults, so the two cannot part."""
        for dotted in self.garage.PREFERENCE_SCHEMA:
            with self.subTest(key=dotted):
                config: dict = {}
                self.garage.set_nested(config, dotted, "value")
        for rejected in ("appearance.no_such_key", "nowhere.key", "appearance",
                         "appearance.glass_mode.deep"):
            with self.subTest(rejected=rejected):
                with self.assertRaises(self.garage.SettingsError):
                    self.garage.set_nested({}, rejected, "value")

    def test_withdrawn_keys_are_not_current_keys(self) -> None:
        """A withdrawn name that came back would be deleted on every load."""
        for section, withdrawn in self.garage.WITHDRAWN_KEYS.items():
            self.assertIn(section, self.garage.FALLBACK_DEFAULTS)
            for name in withdrawn:
                with self.subTest(key=f"{section}.{name}"):
                    self.assertNotIn(f"{section}.{name}", self.garage.PREFERENCE_SCHEMA)


class Routing(BackendTestCase):
    """Every route names something real, and every settable key has one."""

    def steps(self):
        for name, route in self.garage.PREFERENCE_ROUTES.items():
            for step in route:
                yield name, ((step,) if isinstance(step, str) else tuple(step))

    def test_every_settable_key_has_a_route(self) -> None:
        """The silent failure the table exists to end.

        A key with no route saves to preferences.toml and never reaches the
        running desktop -- the setting appears to do nothing until the next
        session start, and nothing anywhere says why.
        """
        for dotted, entry in self.garage.PREFERENCE_SCHEMA.items():
            if entry["kind"] == "bookkeeping":
                continue
            with self.subTest(key=dotted):
                self.assertIn("route", entry)
                self.assertIn(entry["route"], self.garage.PREFERENCE_ROUTES)

    def test_the_stamp_has_no_route(self) -> None:
        """It is bookkeeping, and `set schema.preferences_version` is refused."""
        self.assertNotIn("route",
                         self.garage.PREFERENCE_SCHEMA["schema.preferences_version"])
        with self.assertRaises(self.garage.SettingsError):
            self.garage.apply_changed_preference({}, "schema.preferences_version")

    def test_every_route_step_names_a_real_function(self) -> None:
        for route, step in self.steps():
            name = step[0]
            with self.subTest(route=route, step=name):
                target = getattr(self.garage, name, None)
                self.assertTrue(callable(target),
                                f"{name} is not a function in the backend")

    def test_every_route_step_takes_the_arguments_it_is_given(self) -> None:
        """The arity failure a route table can otherwise only discover in use."""
        for route, step in self.steps():
            name, arguments = step[0], step[1:]
            with self.subTest(route=route, step=name):
                signature = inspect.signature(getattr(self.garage, name))
                # A config, then whatever the route names beside the function.
                signature.bind({}, *arguments)

    def test_a_render_step_never_signals_and_an_apply_step_always_may(self) -> None:
        """The render/apply split, read off the route table.

        render_* writes files and signals nothing; apply_*, push_* and
        run_or_raise are the only things allowed to move the running session. A
        route that reached the compositor through a render_ name would put a
        signal on the `garage render` path, which hypridle's ExecStartPre runs.
        """
        for route, step in self.steps():
            name = step[0]
            with self.subTest(route=route, step=name):
                self.assertTrue(
                    name.startswith(("render_", "apply_", "push_", "reload_"))
                    or name == "run_or_raise",
                    f"{name} is neither a renderer nor an applier")

    def test_every_route_is_reachable(self) -> None:
        """A route nothing points at is dead code that reads as live."""
        used = {entry["route"] for entry in self.garage.PREFERENCE_SCHEMA.values()
                if "route" in entry}
        used |= set(self.garage.SECTION_ROUTES.values())
        self.assertEqual(set(), set(self.garage.PREFERENCE_ROUTES) - used)

    def test_section_routes_name_real_sections_and_real_routes(self) -> None:
        for section, route in self.garage.SECTION_ROUTES.items():
            with self.subTest(section=section):
                self.assertIn(section, self.garage.VALIDATED_SECTIONS)
                self.assertIn(route, self.garage.PREFERENCE_ROUTES)

    def test_an_unknown_key_in_a_named_section_is_refused(self) -> None:
        """appearance and general have always answered by name, and still do."""
        for dotted, message in (
                ("appearance.no_such_key",
                 "Unsupported appearance preference: no_such_key"),
                ("general.no_such_key",
                 "Unsupported general preference: no_such_key"),
                ("nowhere.key", "Unsupported preference section: nowhere")):
            with self.subTest(key=dotted):
                with self.assertRaises(self.garage.SettingsError) as raised:
                    self.garage.apply_changed_preference({}, dotted)
                self.assertEqual(message, str(raised.exception))


class CoercionWording(BackendTestCase):
    """The note's shape is an interface: the journal is grepped for it."""

    def note_for(self, section: str, key: str, value: object) -> list[str]:
        notes: list[str] = []
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        config[section][key] = value
        self.garage.validate_preferences(config, notes)
        return notes

    def test_a_coercion_says_what_it_replaced_and_with_what(self) -> None:
        self.assertEqual(
            ["appearance.border_size 99 is not valid, using 0"],
            self.note_for("appearance", "border_size", 99))

    def test_the_value_is_reported_as_it_was_found(self) -> None:
        """repr, not str: '99' and 99 are different stored values."""
        self.assertEqual(
            ["appearance.border_size '99' is not valid, using 0"],
            self.note_for("appearance", "border_size", "99"))

    def test_a_withdrawn_name_is_carried_across_in_silence(self) -> None:
        """Not a bad value: the same choice under the name it had."""
        for key, withdrawn, current in (("wallpaper_fit", "stretch", "cover"),
                                        ("corner_radius", "small", "normal")):
            with self.subTest(key=key):
                notes: list[str] = []
                config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
                config["appearance"][key] = withdrawn
                self.garage.validate_preferences(config, notes)
                self.assertEqual([], notes)
                self.assertEqual(current, config["appearance"][key])

    def test_the_cross_key_policy_reads_the_other_key(self) -> None:
        """search_custom_url is only refused when custom is the engine chosen."""
        for engine, url, kept in (("custom", "https://e/?q=%s", True),
                                  ("custom", "", True),
                                  ("custom", "no placeholder", False),
                                  ("google", "no placeholder", True)):
            with self.subTest(engine=engine, url=url):
                config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
                config["appearance"]["search_engine"] = engine
                config["appearance"]["search_custom_url"] = url
                self.garage.validate_preferences(config, [])
                self.assertEqual(url if kept else "",
                                 config["appearance"]["search_custom_url"])

    def test_a_normalised_value_is_restated_without_a_note(self) -> None:
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        config["workspaces"]["counts"] = " DP-1 = 4 ,, DP-2=99, =3, X=abc "
        notes: list[str] = []
        self.garage.validate_preferences(config, notes)
        self.assertEqual([], notes)
        self.assertEqual("DP-1=4,DP-2=10", config["workspaces"]["counts"])

    def test_a_withdrawn_key_leaves_the_merged_view(self) -> None:
        config = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        config["appearance"]["highlight_color"] = "#ff0000"
        config["appearance"]["wallpaper"] = "/pictures/one.jpg"
        self.garage.validate_preferences(config, [])
        self.assertNotIn("highlight_color", config["appearance"])
        self.assertNotIn("wallpaper", config["appearance"])

    def test_a_missing_section_is_filled_in_rather_than_raising(self) -> None:
        """A load must never fail: `set` is the only screen that could fix it.

        Every *checked* key comes back at its shipped value. An "unchecked" one
        does not, and that is the documented behaviour rather than an oversight:
        the pass only writes where it has a judgement to make, and the merge with
        layer 1 has already supplied every key by the time load_preferences()
        calls this. Stated as a test because the table now makes the set of
        unchecked keys explicit, and a kind change would move this line.
        """
        config: dict = {}
        notes: list[str] = []
        self.garage.validate_preferences(config, notes)
        self.assertEqual(sorted(self.garage.VALIDATED_SECTIONS), sorted(config))
        for dotted, entry in self.garage.PREFERENCE_SCHEMA.items():
            section, key = dotted.split(".")
            if section not in self.garage.VALIDATED_SECTIONS:
                continue
            with self.subTest(key=dotted):
                if entry["kind"] == "unchecked":
                    self.assertNotIn(key, config[section])
                else:
                    self.assertEqual(entry["default"], config[section][key])


if __name__ == "__main__":
    unittest.main()
