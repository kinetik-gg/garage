"""Layer 2 holds departures, and nothing else.

preferences.toml used to be written as the whole merged configuration, which had
two consequences the schema was designed to avoid:

  * the first settings change a fresh install ever made froze a copy of all ~50
    shipped defaults into layer 2, where they outranked layer 1 forever -- so no
    later change to any shipped default could reach that machine again;
  * a key withdrawn from the schema stayed in the file for good, because every
    save wrote back whatever the load had merged.

v5 is the fix: the stamp, plus the keys whose values differ from the shipped
defaults. Everything here is a property of that -- what a write puts in the file,
what the one-time migration does to a file written by v4, and that neither one
can move an effective value.

The assertions go through the backend's own functions: load_preferences(),
save_preferences(), preference_deltas(), set_nested() and validate_preferences().
`change_preference()` below is main()'s `set` with the apply step left off, which
is the only part of it that needs a compositor.
"""

from __future__ import annotations

import copy
import fcntl
import json
import tomllib
import unittest

from harness import BackendTestCase


def change_preference(garage, key: str, value: object) -> dict:
    """What `garage set KEY VALUE` does, minus the apply, which needs hyprctl.

    Deliberately the same four calls in the same order as main(), so a change to
    what `set` means is a change here too rather than a test that keeps passing
    against code the product no longer runs.
    """
    config = garage.load_preferences()
    garage.set_nested(config, key, value)
    garage.validate_preferences(config)
    garage.save_preferences(config)
    return config


def stored(garage) -> dict:
    """preferences.toml as TOML sees it."""
    with garage.PREFERENCES_PATH.open("rb") as handle:
        return tomllib.load(handle)


def write_stored(garage, text: str) -> bytes:
    garage.PREFERENCES_PATH.parent.mkdir(parents=True, exist_ok=True)
    garage.PREFERENCES_PATH.write_text(text, encoding="utf-8")
    return garage.PREFERENCES_PATH.read_bytes()


def full_document(garage, version: int, departures: dict) -> str:
    """A whole merged configuration, the way every version up to 4 wrote one."""
    document = garage.deep_merge(garage.FALLBACK_DEFAULTS, departures)
    document["schema"] = {"preferences_version": version}
    return garage.dump_toml(document)


def effective_without_migrating(garage) -> dict:
    """The effective configuration of the file as it stands, with no rewrite.

    The merge the loader does, by hand, so a test can hold the *before* of the v5
    migration next to its *after*: load_preferences() would rewrite the file on
    the way past and there would be no before left to compare against.
    """
    defaults = garage.shipped_defaults()
    values = garage.migrate_preference_values(garage.load_toml(garage.PREFERENCES_PATH))
    return garage.validate_preferences(garage.deep_merge(defaults, values))


def values_only(config: dict) -> str:
    """Every effective preference as one comparable string, stamp excluded."""
    return json.dumps({name: values for name, values in config.items() if name != "schema"},
                      sort_keys=True, indent=1)


def flat_keys(document: dict) -> set[str]:
    return {f"{section}.{key}" for section, values in document.items()
            for key in (values if isinstance(values, dict) else {})}


class DeltaWriting(BackendTestCase):
    """What a write puts in the file."""

    def test_reading_a_fresh_install_writes_nothing(self) -> None:
        """No file, and no reason to make one: layer 1 already says everything.

        A load that created the file would also take the bootstrap's GPU gate out
        of play -- it writes preferences.toml only when it does not exist yet.
        """
        self.garage.load_preferences()
        self.assertFalse(self.garage.PREFERENCES_PATH.exists())

    def test_the_first_change_writes_the_stamp_and_the_change(self) -> None:
        change_preference(self.garage, "appearance.accent_color", "red")
        self.assertEqual(
            {"schema": {"preferences_version": self.garage.PREFERENCES_VERSION},
             "appearance": {"accent_color": "red"}},
            stored(self.garage))

    def test_no_shipped_default_is_copied_into_the_file(self) -> None:
        """The fossil, stated as the property it violated."""
        change_preference(self.garage, "appearance.accent_color", "red")
        self.assertEqual({"schema.preferences_version", "appearance.accent_color"},
                         flat_keys(stored(self.garage)))

    def test_a_second_change_joins_the_first(self) -> None:
        change_preference(self.garage, "appearance.accent_color", "red")
        change_preference(self.garage, "lock.lock_timeout", 300)
        self.assertEqual(
            {"schema": {"preferences_version": self.garage.PREFERENCES_VERSION},
             "appearance": {"accent_color": "red"}, "lock": {"lock_timeout": 300}},
            stored(self.garage))

    def test_setting_a_value_back_to_the_default_erases_it(self) -> None:
        """The documented semantic: back to the default means "follow the default".

        Not "pin today's default". A key equal to layer 1 is absent from layer 2,
        so a later release that moves that default moves this machine with it.
        """
        default = self.garage.FALLBACK_DEFAULTS["appearance"]["accent_color"]
        change_preference(self.garage, "appearance.accent_color", "red")
        config = change_preference(self.garage, "appearance.accent_color", default)
        self.assertEqual({"schema": {"preferences_version": self.garage.PREFERENCES_VERSION}},
                         stored(self.garage))
        # Erased from the file, unchanged in effect.
        self.assertEqual(default, config["appearance"]["accent_color"])
        self.assertEqual(default,
                         self.garage.load_preferences()["appearance"]["accent_color"])

    def test_a_write_cannot_move_an_effective_value(self) -> None:
        """load -> save -> load is a fixed point, over a spread of departures."""
        for key, value in (("appearance.theme_mode", "auto"),
                           ("appearance.animation_speed", 1.5),
                           ("appearance.reduce_motion", True),
                           ("input.repeat_rate", 40),
                           ("region.time_format", "12"),
                           ("workspaces.counts", "DP-1=4,DP-2=6")):
            change_preference(self.garage, key, value)
        before = values_only(self.garage.load_preferences())
        self.garage.save_preferences(self.garage.load_preferences())
        self.assertEqual(before, values_only(self.garage.load_preferences()))

    def test_an_unknown_key_is_dropped_with_a_note(self) -> None:
        """Dead weight, and it says so once on the way out.

        Stamped current, so the migration does not run: this is the write-time
        half of the policy.
        """
        write_stored(self.garage, full_document(
            self.garage, self.garage.PREFERENCES_VERSION, {})
            .replace("[general]", "retired_setting = 3\n\n[general]"))
        notes: list[str] = []
        config = self.garage.load_preferences(notes)
        self.assertEqual(3, config["appearance"]["retired_setting"])
        self.garage.save_preferences(config, notes)
        self.assertNotIn("appearance.retired_setting", flat_keys(stored(self.garage)))
        self.assertEqual(["appearance.retired_setting is not a preference this build "
                          "has; dropping it"], notes)

    def test_a_section_the_schema_does_not_have_is_dropped(self) -> None:
        write_stored(self.garage,
                     f"[schema]\npreferences_version = {self.garage.PREFERENCES_VERSION}\n"
                     "\n[nowhere]\nkey = 1\n")
        notes: list[str] = []
        self.garage.save_preferences(self.garage.load_preferences(notes), notes)
        self.assertEqual({"schema": {"preferences_version": self.garage.PREFERENCES_VERSION}},
                         stored(self.garage))
        self.assertEqual(["nowhere is not a preference this build has; dropping it"], notes)

    def test_an_invalid_value_heals_by_disappearing_and_is_reported_once(self) -> None:
        """The coercion and the delta meet here, and they compose.

        validate_preferences() puts the shipped default over a value it cannot
        render, in the merged view. The write then subtracts the shipped defaults
        from that view -- so the coerced key equals its default, is not a
        departure, and leaves the file. It is reported by the coercion and not
        again by the write: a dropped-key note is for a key the *schema* does not
        have, and this one is in the schema with a bad value.
        """
        write_stored(self.garage,
                     f"[schema]\npreferences_version = {self.garage.PREFERENCES_VERSION}\n"
                     '\n[appearance]\nborder_size = 99\naccent_color = "red"\n')
        notes: list[str] = []
        config = self.garage.load_preferences(notes)
        self.garage.set_nested(config, "appearance.theme_mode", "light")
        self.garage.validate_preferences(config, notes)
        self.garage.save_preferences(config, notes)
        self.assertEqual(1, len(notes), notes)
        self.assertIn("appearance.border_size 99 is not valid", notes[0])
        self.assertEqual({"schema.preferences_version", "appearance.accent_color",
                          "appearance.theme_mode"}, flat_keys(stored(self.garage)))
        # Healed for good: the file it left behind loads with nothing to say.
        again: list[str] = []
        self.garage.load_preferences(again)
        self.assertEqual([], again)


class MigrationToDeltas(BackendTestCase):
    """What v5 does to a file that v4 wrote."""

    DEPARTURES = {"appearance": {"accent_color": "purple", "glass_blur": "deep"},
                  "lock": {"lock_timeout": 0}}

    def test_a_full_document_shrinks_to_its_departures(self) -> None:
        write_stored(self.garage, full_document(self.garage, 4, self.DEPARTURES))
        self.garage.load_preferences()
        self.assertEqual(
            {"schema": {"preferences_version": self.garage.PREFERENCES_VERSION},
             "appearance": {"accent_color": "purple", "glass_blur": "deep"},
             "lock": {"lock_timeout": 0}},
            stored(self.garage))

    def test_the_migration_does_not_move_an_effective_value(self) -> None:
        """Byte-for-byte, every preference before the rewrite and after it."""
        write_stored(self.garage, full_document(self.garage, 4, self.DEPARTURES))
        before = values_only(effective_without_migrating(self.garage))
        self.garage.load_preferences()
        self.assertEqual(before, values_only(effective_without_migrating(self.garage)))
        self.assertEqual(before, values_only(self.garage.load_preferences()))

    def test_the_rewrite_happens_once(self) -> None:
        """Stamped current, so a second load has nothing to do and writes nothing."""
        write_stored(self.garage, full_document(self.garage, 4, self.DEPARTURES))
        self.garage.load_preferences()
        self.assertEqual(self.garage.PREFERENCES_VERSION,
                         stored(self.garage)["schema"]["preferences_version"])
        shrunk = self.garage.PREFERENCES_PATH.read_bytes()
        self.garage.load_preferences()
        self.assertEqual(shrunk, self.garage.PREFERENCES_PATH.read_bytes())

    def test_a_file_that_is_already_departures_only_is_left_alone(self) -> None:
        """The bootstrap's GPU gate writes exactly this, comments and all.

        Nothing to shrink, so nothing is written -- dump_toml() cannot carry a
        comment, and rewriting the file would cost the user the explanation of
        why the material is off on this machine. The stamp stays behind with it;
        the step is a dict diff and it costs one to re-run.
        """
        gate = ("# Written once by Garage's bootstrap: integrated graphics only.\n"
                "\n[schema]\npreferences_version = 4\n"
                '\n[appearance]\nglass_mode = "off"\n')
        before = write_stored(self.garage, gate)
        config = self.garage.load_preferences()
        self.assertEqual(before, self.garage.PREFERENCES_PATH.read_bytes())
        self.assertEqual("off", config["appearance"]["glass_mode"])
        # And the first real change is what shrinks and stamps it, keeping the
        # deliberate departure it was written for.
        change_preference(self.garage, "appearance.border_size", 2)
        self.assertEqual(
            {"schema": {"preferences_version": self.garage.PREFERENCES_VERSION},
             "appearance": {"border_size": 2, "glass_mode": "off"}},
            stored(self.garage))

    def test_an_unknown_key_is_dropped_at_migration_time_with_a_note(self) -> None:
        write_stored(self.garage, '[schema]\npreferences_version = 4\n'
                                  '\n[appearance]\naccent_color = "red"\n'
                                  'retired_setting = 3\n')
        notes: list[str] = []
        config = self.garage.load_preferences(notes)
        self.assertEqual({"schema.preferences_version", "appearance.accent_color"},
                         flat_keys(stored(self.garage)))
        self.assertEqual(["appearance.retired_setting is not a preference this build "
                          "has; dropping it"], notes)
        # Reported once. The load continues from what was written, not from what
        # was read, so a save later in the same process has nothing left to say.
        self.garage.save_preferences(config, notes)
        self.assertEqual(1, len(notes), notes)

    def test_the_value_migrations_and_the_shrink_compose(self) -> None:
        """An unstamped file, carrying both older schemas' shapes.

        corner_radius "small" became "normal", which is what the schema now
        ships -- so the rename lands and the key then has nothing to record and
        leaves. The single wallpaper became one per appearance, and both halves
        are departures, so both stay.
        """
        write_stored(self.garage, '[appearance]\ncorner_radius = "small"\n'
                                  'wallpaper = "/pictures/one.jpg"\n')
        config = self.garage.load_preferences()
        self.assertEqual("normal", config["appearance"]["corner_radius"])
        self.assertEqual(
            {"schema": {"preferences_version": self.garage.PREFERENCES_VERSION},
             "appearance": {"wallpaper_light": "/pictures/one.jpg",
                            "wallpaper_dark": "/pictures/one.jpg"}},
            stored(self.garage))

    def test_the_rewrite_stands_aside_while_a_writer_holds_the_lock(self) -> None:
        """The load path may not wait on PREFERENCES_LOCK, so it does not.

        `set lock.*` restarts hypridle while holding that lock, and hypridle's
        ExecStartPre re-enters this binary to render -- a blocking acquire on the
        load path would deadlock the restart. Non-blocking, and skipped when it is
        held, which is safe because whoever holds it is a writer that emits
        departures only. The effective configuration is right either way.
        """
        before = write_stored(self.garage, full_document(self.garage, 4, self.DEPARTURES))
        self.garage.PREFERENCES_LOCK.parent.mkdir(parents=True, exist_ok=True)
        with self.garage.PREFERENCES_LOCK.open("a+", encoding="utf-8") as held:
            fcntl.flock(held.fileno(), fcntl.LOCK_EX)
            config = self.garage.load_preferences()
            self.assertEqual(before, self.garage.PREFERENCES_PATH.read_bytes())
        self.assertEqual("purple", config["appearance"]["accent_color"])
        self.assertEqual(0, config["lock"]["lock_timeout"])
        # Released, so the next load finishes the job.
        self.garage.load_preferences()
        self.assertEqual(self.garage.PREFERENCES_VERSION,
                         stored(self.garage)["schema"]["preferences_version"])


class FossilPrevention(BackendTestCase):
    """A shipped default that moves has to reach a machine that already exists."""

    def shipped(self, **appearance: object) -> dict:
        """Install a layer 1 that differs from the compiled-in copy.

        The point of the whole change, and the one thing FALLBACK_DEFAULTS cannot
        stand in for: this is a release changing a default under an install.
        """
        defaults = copy.deepcopy(self.garage.FALLBACK_DEFAULTS)
        defaults["appearance"].update(appearance)
        self.garage.DEFAULTS_PATH.parent.mkdir(parents=True, exist_ok=True)
        self.garage.DEFAULTS_PATH.write_text(self.garage.dump_toml(defaults),
                                             encoding="utf-8")
        return defaults

    def test_a_changed_shipped_default_reaches_an_existing_install(self) -> None:
        self.shipped(accent_color="teal")
        write_stored(self.garage,
                     f"[schema]\npreferences_version = {self.garage.PREFERENCES_VERSION}\n"
                     '\n[appearance]\ntheme_mode = "light"\n')
        config = self.garage.load_preferences()
        # The new default arrives, because no copy of the old one is in the way.
        self.assertEqual("teal", config["appearance"]["accent_color"])
        # And the user's own choice is untouched by it.
        self.assertEqual("light", config["appearance"]["theme_mode"])

    def test_the_merge_base_is_the_shipped_file_not_the_compiled_copy(self) -> None:
        """A save subtracts what the load added, or it invents a fossil.

        The effective accent here is "teal" because layer 1 says so, and layer 2
        must come out of the save without it. Subtracting FALLBACK_DEFAULTS
        instead -- which still ships "blue" -- would write accent_color = "teal"
        as though the user had chosen it, and the next release to move that
        default could never reach this machine.
        """
        self.shipped(accent_color="teal")
        change_preference(self.garage, "lock.lock_timeout", 300)
        self.assertEqual({"schema.preferences_version", "lock.lock_timeout"},
                         flat_keys(stored(self.garage)))

    def test_a_full_document_shrinks_against_the_shipped_file(self) -> None:
        """The migration reads layer 1 from disk for the same reason."""
        defaults = self.shipped(accent_color="teal")
        document = self.garage.deep_merge(defaults, {"appearance": {"border_size": 3}})
        document["schema"] = {"preferences_version": 4}
        write_stored(self.garage, self.garage.dump_toml(document))
        self.garage.load_preferences()
        self.assertEqual({"schema.preferences_version", "appearance.border_size"},
                         flat_keys(stored(self.garage)))


class DeltaComputation(BackendTestCase):
    """preference_deltas() and same_default(), directly."""

    def test_a_key_equal_to_its_default_is_not_a_departure(self) -> None:
        self.assertEqual({}, self.garage.preference_deltas(
            {"section": {"key": "value"}}, {"section": {"key": "value"}}))

    def test_a_key_the_defaults_do_not_have_is_reported_not_kept(self) -> None:
        dropped: list[str] = []
        self.assertEqual({}, self.garage.preference_deltas(
            {"section": {"gone": 1}}, {"section": {"key": 2}}, dropped))
        self.assertEqual(["section.gone"], dropped)

    def test_a_bool_is_never_the_same_value_as_a_number(self) -> None:
        """True == 1 in Python, and the two are not the same stored value.

        Dropping a stored `true` against a default of `1` would hand the
        renderers an int where the file said bool.
        """
        self.assertEqual({"s": {"k": True}},
                         self.garage.preference_deltas({"s": {"k": True}}, {"s": {"k": 1}}))
        self.assertEqual({"s": {"k": 0}},
                         self.garage.preference_deltas({"s": {"k": 0}}, {"s": {"k": False}}))
        self.assertEqual({}, self.garage.preference_deltas({"s": {"k": True}},
                                                          {"s": {"k": True}}))

    def test_an_int_equal_to_a_float_default_is_the_default(self) -> None:
        """1 == 1.0, and there the numbers are the same value.

        The schema ships pointer_sensitivity as 0.0 and the pane sends JSON `0`
        for a slider at zero, so this is the ordinary case rather than a corner:
        keeping it would pin a copy of the default in layer 2 over a decimal
        point. Layer 1 owns the type as well as the value.
        """
        self.assertEqual({}, self.garage.preference_deltas({"s": {"k": 0}},
                                                          {"s": {"k": 0.0}}))
        self.assertEqual({"s": {"k": 1}}, self.garage.preference_deltas(
            {"s": {"k": 1}}, {"s": {"k": 0.0}}))

    def test_sections_are_compared_recursively(self) -> None:
        """Two levels deep today, so a third must not be compared shallowly."""
        self.assertEqual({"a": {"b": {"d": 2}}}, self.garage.preference_deltas(
            {"a": {"b": {"c": 1, "d": 2}}}, {"a": {"b": {"c": 1, "d": 9}}}))

    def test_a_value_where_a_section_belongs_is_dropped(self) -> None:
        dropped: list[str] = []
        self.assertEqual({}, self.garage.preference_deltas(
            {"a": 1}, {"a": {"b": 2}}, dropped))
        self.assertEqual(["a"], dropped)

    def test_the_shipped_key_order_is_kept(self) -> None:
        """So the file reads in the order the documented defaults file does."""
        defaults = {"s": {"one": 1, "two": 2, "three": 3}}
        deltas = self.garage.preference_deltas({"s": {"three": 9, "one": 9}}, defaults)
        self.assertEqual(["one", "three"], list(deltas["s"]))

    def test_the_document_always_carries_the_stamp(self) -> None:
        """Diffed away, it would leave a file that reads as version 1 -- and every
        migration would replay over it on the next load."""
        document = self.garage.preference_document(
            copy.deepcopy(self.garage.FALLBACK_DEFAULTS), self.garage.FALLBACK_DEFAULTS)
        self.assertEqual({"schema": {"preferences_version":
                                     self.garage.PREFERENCES_VERSION}}, document)


if __name__ == "__main__":
    unittest.main()
