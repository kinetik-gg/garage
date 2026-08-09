"""Schema coherence: FALLBACK_DEFAULTS must equal preferences.defaults.toml.

The backend keeps the preference schema twice. `preferences.defaults.toml` is
layer 1 -- the shipped file every install merges on top of -- and
FALLBACK_DEFAULTS in the backend is what `load_toml(DEFAULTS_PATH, ...)` falls
back to when that file is missing, *and* what `validate_preferences()` coerces
every stored value against (it reads `FALLBACK_DEFAULTS[section][key]` for the
default, not the TOML). Two copies of one schema, agreeing today only because
whoever edited one remembered to edit the other.

A key present in only one of them is a live defect either way round:

  * TOML-only  -> validate_preferences() raises KeyError on a key it is
    supposed to be validating, and `set` refuses the key outright
    (set_nested() gates on FALLBACK_DEFAULTS).
  * Python-only -> a fresh install never gets the key in its defaults file, so
    the documented, hand-editable layer 1 is missing a setting that exists.

A key whose *value* differs is worse than either, because nothing errors: the
install behaves one way and the fallback path behaves another.

So this pins all three -- key sets both directions, value types, and values.
"""

from __future__ import annotations

from pathlib import Path
import tomllib
import unittest

from harness import (DEFAULTS_TOML, PATH_CONSTANTS, SYSTEM_PATHS, BackendTestCase,
                     assert_confined)


def flatten(data: dict, prefix: str = "") -> dict[str, object]:
    """Every leaf of a nested mapping, as dotted-path -> value.

    Recursive rather than two-level: the schema is two levels deep today, and a
    section gaining a subsection must not silently stop being compared.
    """
    flat: dict[str, object] = {}
    for key, value in data.items():
        path = f"{prefix}{key}"
        if isinstance(value, dict):
            nested = flatten(value, f"{path}.")
            if nested:
                flat.update(nested)
            else:
                # An empty section is still a schema fact.
                flat[path] = {}
        else:
            flat[path] = value
    return flat


def type_name(value: object) -> str:
    """The TOML-visible type of a value, with bool split out from int.

    bool is a subclass of int in Python, so `True` and `1` compare equal and
    have to be told apart by name or a `true` in one file and a `1` in the other
    would pass.
    """
    if isinstance(value, bool):
        return "bool"
    return type(value).__name__


class SchemaCoherence(BackendTestCase):
    def setUp(self) -> None:
        super().setUp()
        # Located from this file, not from $HOME: the point is to check the
        # defaults file in *this checkout*, and the backend's own DEFAULTS_PATH
        # has been redirected into the scratch root.
        self.assertTrue(DEFAULTS_TOML.is_file(), f"missing {DEFAULTS_TOML}")
        with DEFAULTS_TOML.open("rb") as handle:
            self.shipped = flatten(tomllib.load(handle))
        self.fallback = flatten(self.garage.FALLBACK_DEFAULTS)

    def test_no_key_only_in_python(self) -> None:
        """Every FALLBACK_DEFAULTS key is shipped in preferences.defaults.toml."""
        missing = sorted(set(self.fallback) - set(self.shipped))
        self.assertEqual([], missing,
                         "in FALLBACK_DEFAULTS but not in preferences.defaults.toml")

    def test_no_key_only_in_toml(self) -> None:
        """Every shipped key exists in FALLBACK_DEFAULTS.

        validate_preferences() indexes FALLBACK_DEFAULTS[section][key] directly,
        so a TOML-only key is a KeyError on the load path.
        """
        missing = sorted(set(self.shipped) - set(self.fallback))
        self.assertEqual([], missing,
                         "in preferences.defaults.toml but not in FALLBACK_DEFAULTS")

    def test_value_types_agree(self) -> None:
        """Per key, the two copies declare the same type."""
        mismatched = {
            key: (type_name(self.fallback[key]), type_name(self.shipped[key]))
            for key in sorted(set(self.fallback) & set(self.shipped))
            if type_name(self.fallback[key]) != type_name(self.shipped[key])
        }
        self.assertEqual({}, mismatched,
                         "key -> (FALLBACK_DEFAULTS type, defaults.toml type)")

    def test_values_agree(self) -> None:
        """Per key, the two copies declare the same default value."""
        mismatched = {
            key: (self.fallback[key], self.shipped[key])
            for key in sorted(set(self.fallback) & set(self.shipped))
            if self.fallback[key] != self.shipped[key]
        }
        self.assertEqual({}, mismatched,
                         "key -> (FALLBACK_DEFAULTS value, defaults.toml value)")

    def test_preferences_version_constant_is_resolved(self) -> None:
        """The stamp is the constant, and the shipped file carries that number.

        FALLBACK_DEFAULTS holds `PREFERENCES_VERSION` as an expression while the
        TOML holds a literal, which is the one place in the schema where a bump
        can only be caught by comparing against the constant itself.
        """
        self.assertEqual(self.garage.PREFERENCES_VERSION,
                         self.fallback["schema.preferences_version"])
        self.assertEqual(self.garage.PREFERENCES_VERSION,
                         self.shipped["schema.preferences_version"])

    def test_sections_are_the_same_set(self) -> None:
        """No whole section exists on one side only.

        Distinct from the key checks: an empty or entirely-new section would
        show up here as one clear failure rather than as a list of keys.
        """
        shipped_sections = {key.split(".")[0] for key in self.shipped}
        self.assertEqual(sorted(self.garage.FALLBACK_DEFAULTS), sorted(shipped_sections))


class ScratchConfinement(BackendTestCase):
    """The tests' own safety net: nothing may reach the developer's desktop."""

    def test_every_path_constant_is_inside_the_scratch_root(self) -> None:
        # load_backend() already asserts this; repeated here so the property has
        # a named test rather than being an invisible precondition.
        assert_confined(self.garage, self.root)

    def test_path_constants_are_enumerated(self) -> None:
        """Any new module-level Path in the backend joins the confinement list.

        Otherwise a future path constant could point at the real $HOME and the
        check above would skip it silently.
        """
        found = {name for name, value in vars(self.garage).items()
                 if name.isupper() and isinstance(value, Path)}
        self.assertEqual(set(), found - set(PATH_CONSTANTS) - set(SYSTEM_PATHS),
                         "module-level Path constants missing from "
                         "harness.PATH_CONSTANTS -- add them so they are "
                         "checked, or to SYSTEM_PATHS if they are read-only "
                         "system locations")

    def test_system_paths_are_outside_the_home_directory(self) -> None:
        """A path excused from confinement must really be a system path."""
        for name in SYSTEM_PATHS:
            value = getattr(self.garage, name)
            with self.subTest(constant=name):
                self.assertTrue(value.is_absolute())
                self.assertFalse(str(value).startswith(str(self.root)))
                self.assertFalse(str(value).startswith(str(Path.home())))

    def test_loading_the_backend_does_not_run_main(self) -> None:
        """The script is loaded under a module name that is not "__main__", so
        its `raise SystemExit(main(sys.argv))` tail stays inert."""
        self.assertNotEqual("__main__", self.garage.__name__)
        self.assertTrue(callable(self.garage.main))

    def test_loading_the_backend_creates_nothing(self) -> None:
        """Import time is path arithmetic and nothing else.

        If a constant ever grows an mkdir beside it, the whole harness stops
        being safe -- so the absence of side effects is a test, not a comment.
        """
        self.assertFalse(self.garage.GENERATED.exists())
        self.assertFalse(self.garage.PREFERENCES_PATH.exists())
        self.assertFalse(self.garage.KEYBINDINGS_PATH.exists())
        self.assertFalse(self.garage.PREFERENCES_LOCK.exists())

    def test_real_config_root_is_untouched(self) -> None:
        """The scratch root, not ~/.config/garage, is what the backend holds."""
        self.assertEqual(self.root / ".config" / "garage", self.garage.ROOT)
        self.assertEqual(self.root / ".local" / "state" / "garage",
                         self.garage.STATE_ROOT)


if __name__ == "__main__":
    unittest.main()
