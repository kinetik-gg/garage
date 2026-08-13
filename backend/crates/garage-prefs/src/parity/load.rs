//! The read path, case by case, against the Python's own answers.

use crate::error::PrefsError;
use crate::load::load_preferences;
use crate::lock::PrefLock;
use crate::testing::World;

/// A load in a stowed world, reporting the notes it produced and the file it left behind.
fn load(label: &str, planted: Option<&str>) -> (Vec<String>, Option<String>) {
    let world = World::stowed(label);
    if let Some(text) = planted {
        world.plant_preferences(text);
    }
    let mut sink = Vec::new();
    load_preferences(world.paths(), Some(&mut sink)).expect("the load succeeds");
    (sink, world.preferences_file())
}

/// A load that is expected to leave the file exactly as it found it.
fn load_leaving_it_alone(label: &str, planted: &str) -> Vec<String> {
    let (notes, after) = load(label, Some(planted));
    assert_eq!(after.as_deref(), Some(planted), "{label} rewrote the file");
    notes
}

/// A load that is expected to rewrite the file.
fn load_rewriting(label: &str, planted: &str, expected: &str) -> Vec<String> {
    let (notes, after) = load(label, Some(planted));
    assert_eq!(after.as_deref(), Some(expected), "{label}");
    notes
}

/// The stamp and nothing else: a machine sitting on factory state.
const FACTORY: &str = "[schema]\npreferences_version = 5\n";
/// The stamp and one departure, which is what one changed setting costs.
const ONE_DEPARTURE: &str =
    "[schema]\npreferences_version = 5\n\n[appearance]\naccent_color = \"red\"\n";

#[test]
fn a_fresh_install_is_not_given_a_file() {
    let (notes, after) = load("fresh", None);
    assert!(notes.is_empty());
    assert_eq!(after, None, "a read path must not invent a user-owned file");
}

/// v2's rename and v5's shrink, composed: `small` becomes `normal`, which is what this
/// build ships, so it leaves the file entirely and only the real departure is written.
#[test]
fn v1_corner_radius_is_renamed_and_then_shrunk_away() {
    let notes = load_rewriting(
        "v1-corner-small",
        "[appearance]\ncorner_radius = \"small\"\naccent_color = \"red\"\n",
        ONE_DEPARTURE,
    );
    assert!(notes.is_empty());
}

/// The case the Python is most easily got wrong on. `normal` migrates to `large`, `large`
/// is a departure, and the departures the file already holds are *the same set* it would be
/// rewritten to -- so the comparison says "no change" and the file is left as it is,
/// unstamped, with the rename replaying (idempotently) on every load.
#[test]
fn v1_corner_radius_normal_migrates_without_the_file_ever_being_stamped() {
    let planted = "[appearance]\ncorner_radius = \"normal\"\n";
    let notes = load_leaving_it_alone("v1-corner-normal", planted);
    assert!(notes.is_empty());
    let world = World::stowed("v1-corner-normal-effective");
    world.plant_preferences(planted);
    let preferences = load_preferences(world.paths(), None).expect("the load succeeds");
    assert_eq!(
        preferences.appearance.corner_radius.to_string(),
        "large",
        "the effective value is the migrated one however the file reads"
    );
}

#[test]
fn v3_split_wallpapers_are_departures_so_the_file_is_left_alone() {
    for (label, planted) in [
        (
            "v1-wallpaper-split",
            "[appearance]\nwallpaper = \"/pic.png\"\nwallpaper_source = \"color\"\n\
             wallpaper_color = \"#010203\"\n",
        ),
        (
            "v2-wallpaper-split",
            "[schema]\npreferences_version = 2\n\n[appearance]\n\
             wallpaper = \"/pic.png\"\nwallpaper_dark = \"/kept.png\"\n",
        ),
    ] {
        assert!(load_leaving_it_alone(label, planted).is_empty());
    }
}

#[test]
fn a_v3_file_is_past_the_rename_so_normal_is_simply_the_default() {
    let notes = load_rewriting(
        "v3-corner",
        "[schema]\npreferences_version = 3\n\n[appearance]\ncorner_radius = \"normal\"\n",
        FACTORY,
    );
    assert!(notes.is_empty());
}

/// A v4 file that is *already* departures-only is left exactly as it is, stamp and all --
/// the rewrite would change nothing, and rewriting it would cost the user any comments.
#[test]
fn a_v4_file_that_is_already_departures_only_keeps_its_old_stamp() {
    let notes = load_leaving_it_alone(
        "v4-departures-only",
        "[schema]\npreferences_version = 4\n\n[appearance]\naccent_color = \"red\"\n",
    );
    assert!(notes.is_empty());
}

#[test]
fn a_hand_written_file_keeps_its_comments() {
    let notes = load_leaving_it_alone(
        "v5-comments",
        "# hand written, keep me\n[schema]\npreferences_version = 5\n\n\
         [appearance]\n# why red\naccent_color = \"red\"\n",
    );
    assert!(notes.is_empty());
}

/// The fossil the whole of v5 exists to clear: a whole merged configuration, the way every
/// version up to 4 wrote one, shrunk to the one setting that was ever changed.
#[test]
fn a_v4_full_document_is_shrunk_to_its_departures() {
    let planted = full_document(4, &[("appearance", "accent_color", "\"red\"")]);
    let notes = load_rewriting("v4-full", &planted, ONE_DEPARTURE);
    assert!(notes.is_empty());
}

#[test]
fn a_v1_full_document_is_migrated_and_shrunk_to_nothing() {
    let planted = full_document(1, &[("appearance", "corner_radius", "\"small\"")]);
    let notes = load_rewriting("v1-full", &planted, FACTORY);
    assert!(notes.is_empty());
}

#[test]
fn a_section_the_schema_does_not_have_is_reported_and_dropped() {
    let mut planted = full_document(4, &[("appearance", "accent_color", "\"red\"")]);
    planted.push_str("\n[bogus]\nfoo = \"bar\"\n");
    let notes = load_rewriting("v4-full-bogus", &planted, ONE_DEPARTURE);
    assert_eq!(
        notes,
        ["bogus is not a preference this build has; dropping it"]
    );
}

#[test]
fn an_unknown_key_is_reported_and_dropped_at_migration_time() {
    let notes = load_rewriting(
        "v4-unknown-key",
        "[schema]\npreferences_version = 4\n\n[appearance]\naccent_color = \"red\"\n\
         not_a_key = 1\n",
        ONE_DEPARTURE,
    );
    assert_eq!(
        notes,
        ["appearance.not_a_key is not a preference this build has; dropping it"]
    );
}

/// The stamp is what stops the migration running, so a file that carries one keeps its
/// unknown key until something writes it out again. Reported nowhere, exactly as in the
/// Python.
#[test]
fn an_unknown_key_in_a_current_file_is_left_where_it_is() {
    let notes = load_leaving_it_alone(
        "v5-unknown-key",
        "[schema]\npreferences_version = 5\n\n[appearance]\naccent_color = \"red\"\n\
         not_a_key = 1\n",
    );
    assert!(notes.is_empty());
}

#[test]
fn a_withdrawn_key_leaves_the_file_with_the_same_note() {
    let notes = load_rewriting(
        "v4-withdrawn",
        "[schema]\npreferences_version = 4\n\n[appearance]\nhighlight_color = \"#ff0000\"\n",
        FACTORY,
    );
    assert_eq!(
        notes,
        ["appearance.highlight_color is not a preference this build has; dropping it"]
    );
}

/// A table where a scalar belongs is not a preference at all -- it cannot be merged, so it
/// is dropped with the same note rather than coerced.
#[test]
fn a_table_where_a_value_belongs_is_dropped_rather_than_coerced() {
    let notes = load_rewriting(
        "inline-table",
        "[schema]\npreferences_version = 4\n\n[appearance]\naccent_color = {a = 1}\n",
        FACTORY,
    );
    assert_eq!(
        notes,
        ["appearance.accent_color is not a preference this build has; dropping it"]
    );
}

/// Same, one level up: `appearance = "hi"` is a section-shaped hole.
#[test]
fn a_section_that_is_not_a_table_is_dropped_whole() {
    let notes = load_rewriting(
        "section-scalar",
        "appearance = \"hi\"\n\n[schema]\npreferences_version = 4\n",
        FACTORY,
    );
    assert_eq!(
        notes,
        ["appearance is not a preference this build has; dropping it"]
    );
}

/// An unusable value is a *departure*, so the compaction leaves it in the file: it is
/// corrected in memory, reported, and only reaches the file on the next save.
#[test]
fn an_invalid_value_is_reported_but_stays_in_the_file() {
    let notes = load_leaving_it_alone(
        "v4-invalid",
        "[schema]\npreferences_version = 4\n\n[appearance]\ncorner_radius = \"huge\"\n\
         border_size = 99\n",
    );
    assert_eq!(
        notes,
        [
            "appearance.corner_radius 'huge' is not valid, using 'normal'",
            "appearance.border_size 99 is not valid, using 0",
        ]
    );
}

/// `1 == 1.0` in Python, and the schema ships `pointer_sensitivity` as a float. A UI that
/// sends JSON `0` must not pin a copy of the default in layer 2 over a decimal point.
#[test]
fn an_int_where_a_float_ships_is_the_default_and_leaves_the_file() {
    let notes = load_rewriting(
        "int-for-float",
        "[schema]\npreferences_version = 4\n\n[input]\npointer_sensitivity = 0\n",
        FACTORY,
    );
    assert!(notes.is_empty());
}

/// The other direction: `True == 1` too, but a bool is a different *kind* of value, so it
/// stays a departure and is reported by the coercion pass instead.
#[test]
fn a_bool_where_an_int_ships_is_a_departure_and_is_reported() {
    let notes = load_leaving_it_alone(
        "bool-for-int",
        "[schema]\npreferences_version = 4\n\n[appearance]\nborder_size = true\n",
    );
    assert_eq!(notes, ["appearance.border_size True is not valid, using 0"]);
}

#[test]
fn a_mangled_stamp_is_older_than_current_rather_than_fatal() {
    for (label, stamp) in [
        ("stamp-string", "\"5\""),
        ("stamp-bool", "true"),
        ("stamp-float", "5.0"),
    ] {
        let planted = format!(
            "[schema]\npreferences_version = {stamp}\n\n[appearance]\naccent_color = \"red\"\n"
        );
        // Departures-only already, so the rewrite would change nothing and the mangled
        // stamp survives -- which is the same outcome the Python reaches.
        assert!(load_leaving_it_alone(label, &planted).is_empty());
    }
}

#[test]
fn a_stamp_from_the_future_stops_every_migration() {
    let notes = load_leaving_it_alone(
        "future-stamp",
        "[schema]\npreferences_version = 99\n\n[appearance]\naccent_color = \"red\"\n",
    );
    assert!(notes.is_empty());
}

/// A hand edit can put a TOML time or a `nan` where a string or a float belongs. Both are
/// reported by the coercion pass, with the `repr` Python's f-string would produce -- and
/// neither reaches the emitter, because both are departures and the file is left alone.
#[test]
fn an_exotic_scalar_is_reported_the_way_python_reprs_it() {
    let notes = load_leaving_it_alone(
        "toml-time",
        "[schema]\npreferences_version = 4\n\n[appearance]\ntheme_light_at = 07:00:00\n",
    );
    assert_eq!(
        notes,
        ["appearance.theme_light_at datetime.time(7, 0) is not valid, using '07:00'"]
    );
    let notes = load_leaving_it_alone(
        "nan-float",
        "[schema]\npreferences_version = 4\n\n[appearance]\nanimation_speed = nan\n",
    );
    assert_eq!(
        notes,
        ["appearance.animation_speed nan is not valid, using 1.0"]
    );
}

/// The same two values, with a second key that *does* have to leave the file. Now the
/// rewrite happens, the emitter is reached, and it refuses them with the Python's own
/// `SettingsError` text.
#[test]
fn an_exotic_scalar_the_emitter_cannot_write_fails_the_load() {
    for (label, line, message) in [
        (
            "nan-rewrite",
            "animation_speed = nan",
            "Non-finite numbers are not supported",
        ),
        (
            "time-rewrite",
            "theme_light_at = 07:00:00",
            "Unsupported TOML value: datetime.time(7, 0)",
        ),
        (
            "date-rewrite",
            "theme_light_at = 1979-05-27",
            "Unsupported TOML value: datetime.date(1979, 5, 27)",
        ),
    ] {
        let world = World::stowed(label);
        world.plant_preferences(&format!(
            "[schema]\npreferences_version = 4\n\n[appearance]\n{line}\n\
             corner_radius = \"normal\"\n"
        ));
        let error = load_preferences(world.paths(), None).expect_err("the emitter refuses it");
        assert_eq!(error.to_string(), message, "{label}");
    }
}

#[test]
fn a_file_that_does_not_parse_is_named_and_refused() {
    let world = World::stowed("corrupt");
    world.plant_preferences("this is not toml\n");
    let error = load_preferences(world.paths(), None).expect_err("a broken file is refused");
    assert!(
        matches!(&error, PrefsError::Unreadable { file, .. } if file == "preferences.toml"),
        "{error}"
    );
    assert_eq!(
        world.preferences_file().as_deref(),
        Some("this is not toml\n"),
        "the user's file is left exactly as it is"
    );
}

/// The load path's one write, skipped because somebody else is between their own read and
/// their own write. Two opens in one process contend, because `flock` belongs to the open
/// file description -- so this is the real thing rather than a simulation of it.
#[test]
fn the_compaction_is_skipped_while_the_lock_is_held() {
    let planted = "[schema]\npreferences_version = 4\n\n[appearance]\n\
                   accent_color = \"red\"\ncorner_radius = \"normal\"\n";
    let world = World::stowed("held-lock");
    world.plant_preferences(planted);
    let held = PrefLock::acquire(world.paths()).expect("the lock is free");
    let mut sink = Vec::new();
    let preferences =
        load_preferences(world.paths(), Some(&mut sink)).expect("the load survives the skip");
    assert_eq!(world.preferences_file().as_deref(), Some(planted));
    assert!(sink.is_empty());
    assert_eq!(
        preferences.appearance.accent_color.to_string(),
        "red",
        "the effective configuration is correct either way; only the file's shape waits"
    );
    drop(held);

    // And with the lock free, the same file is compacted.
    let notes = load_rewriting("free-lock", planted, ONE_DEPARTURE);
    assert!(notes.is_empty());
}

/// Layer 1 missing entirely is the ordinary state of an unstowed checkout, and the Python
/// falls back to its compiled copy without a word.
#[test]
fn a_missing_defaults_file_changes_nothing_the_user_can_see() {
    let world = World::new("unstowed");
    world.plant_preferences(ONE_DEPARTURE);
    let mut sink = Vec::new();
    let preferences = load_preferences(world.paths(), Some(&mut sink)).expect("the load succeeds");
    assert!(sink.is_empty());
    assert_eq!(preferences.appearance.accent_color.to_string(), "red");
    assert_eq!(world.preferences_file().as_deref(), Some(ONE_DEPARTURE));
}

/// A whole merged configuration, the way every version up to 4 wrote one: layer 1 in full,
/// the given overrides on top, and the stamp.
///
/// Built from the shipped file rather than from a checked-in copy of one, so it cannot
/// drift from the schema. Its *key order* is the file's rather than the Python helper's
/// schema-table order; the outcome is identical either way, which was checked against the
/// Python both ways round before this was written down.
fn full_document(version: i64, overrides: &[(&str, &str, &str)]) -> String {
    let mut table: toml::Table = crate::testing::DEFAULTS.parse().expect("layer 1 parses");
    for (section, key, value) in overrides {
        let parsed: toml::Table = format!("v = {value}").parse().expect("the override parses");
        if let Some(section) = table.get_mut(*section).and_then(toml::Value::as_table_mut) {
            section.insert(
                (*key).to_owned(),
                parsed.get("v").expect("v is there").clone(),
            );
        }
    }
    let mut schema = toml::Table::new();
    schema.insert(
        "preferences_version".to_owned(),
        toml::Value::Integer(version),
    );
    table.insert("schema".to_owned(), toml::Value::Table(schema));
    crate::doc::emit_document(&table).expect("layer 1 is writable")
}

/// Three shapes where the Python does not survive the file at all.
///
/// Each is a hand edit reaching a Python expression with no guard on it, and each takes the
/// process down with a traceback rather than a message: `main()` catches `SettingsError`,
/// `OSError`, `ValueError` and `JSONDecodeError`, and none of these is one of those. They
/// are listed here as *divergences*, with the Python's exception written down beside what
/// this port does instead, because a port that reproduced them would be porting a crash.
mod divergences {
    use super::{load, FACTORY};

    /// Python: `TypeError: 'int' object does not support item assignment`, from
    /// `stored.setdefault("schema", {})["preferences_version"] = ...`.
    #[test]
    fn a_top_level_schema_that_is_not_a_table_does_not_crash_the_load() {
        let planted = "schema = 5\n\n[appearance]\naccent_color = \"red\"\n";
        let (notes, after) = load("schema-scalar", Some(planted));
        assert!(notes.is_empty());
        assert_eq!(after.as_deref(), Some(planted));
    }

    /// Python: `AttributeError: 'str' object has no attribute 'get'`, from
    /// `validate_preferences()` reaching a merged section that is a string. Only the
    /// *stamped* file gets there -- an unstamped one is dropped by the compaction first,
    /// which is the `a_section_that_is_not_a_table_is_dropped_whole` case above.
    #[test]
    fn a_stamped_section_that_is_not_a_table_does_not_crash_the_load() {
        let planted = "appearance = \"hi\"\n\n[schema]\npreferences_version = 5\n";
        let (notes, after) = load("section-scalar-v5", Some(planted));
        assert!(notes.is_empty());
        assert_eq!(after.as_deref(), Some(planted));
    }

    /// Python: `TypeError: cannot use 'list' as a set element`, from the enum check doing
    /// `value in {...}` with an unhashable value. Here it is an ordinary coercion.
    #[test]
    fn an_array_where_a_scalar_belongs_is_coerced_rather_than_crashing() {
        let planted =
            "[schema]\npreferences_version = 4\n\n[appearance]\naccent_color = [\"a\", \"b\"]\n";
        let (notes, after) = load("array-value", Some(planted));
        assert_eq!(
            notes,
            ["appearance.accent_color ['a', 'b'] is not valid, using 'blue'"]
        );
        assert_eq!(after.as_deref(), Some(planted));
        assert_ne!(after.as_deref(), Some(FACTORY));
    }
}
