//! The write path, case by case, against the Python's own answers.

use garage_core::schema::notes::Notes;
use garage_core::schema::{PreferenceKey, Preferences};

use crate::load::load_preferences;
use crate::lock::PrefLock;
use crate::save::save_preferences;
use crate::testing::{listing, World};

/// Load, change the given keys, save -- `main()`'s `set` branch with the apply left off,
/// which is the only part of it that needs a compositor. All of it under one hold of the
/// lock, exactly as that branch holds it.
fn set_and_save(world: &World, changes: &[(PreferenceKey, &str)]) -> Vec<String> {
    let mut sink = Vec::new();
    let mut config = load_preferences(world.paths(), Some(&mut sink)).expect("the load succeeds");
    let lock = PrefLock::acquire(world.paths()).expect("the lock is free");
    let mut notes = Notes::new();
    for (key, value) in changes {
        let parsed: toml::Table = format!("v = {value}").parse().expect("the value parses");
        config
            .set(*key, parsed.get("v").expect("v is there"), &mut notes)
            .expect("the key is settable");
    }
    save_preferences(world.paths(), &config, &lock, Some(&mut sink)).expect("the save succeeds");
    sink.extend(notes.as_slice().iter().cloned());
    sink
}

const FACTORY: &str = "[schema]\npreferences_version = 5\n";

#[test]
fn the_first_change_writes_the_stamp_and_the_change() {
    let world = World::stowed("save-one");
    let notes = set_and_save(&world, &[(PreferenceKey::AccentColor, "\"red\"")]);
    assert!(notes.is_empty());
    assert_eq!(
        world.preferences_file().as_deref(),
        Some("[schema]\npreferences_version = 5\n\n[appearance]\naccent_color = \"red\"\n")
    );
}

/// No shipped default is copied into the file -- the fossil, stated as the property it
/// violated. A save that changed nothing writes the stamp and nothing else.
#[test]
fn saving_an_unchanged_configuration_writes_the_stamp_alone() {
    let world = World::stowed("save-nothing");
    assert!(set_and_save(&world, &[]).is_empty());
    assert_eq!(world.preferences_file().as_deref(), Some(FACTORY));
}

/// Setting a value back to the shipped default *erases* the delta rather than pinning it.
/// That is the intended meaning -- "follow the shipped default again" -- under a schema
/// whose defaults are expected to move.
#[test]
fn setting_a_value_back_to_the_default_erases_the_departure() {
    let world = World::stowed("save-back");
    world.plant_preferences(
        "[schema]\npreferences_version = 5\n\n[appearance]\naccent_color = \"red\"\n",
    );
    assert!(set_and_save(&world, &[(PreferenceKey::AccentColor, "\"blue\"")]).is_empty());
    assert_eq!(world.preferences_file().as_deref(), Some(FACTORY));
}

/// The schema ships `pointer_sensitivity` as `0.0`, and a UI that sends JSON `0` stores an
/// int. Treating that as a departure would pin a copy of the default in layer 2 forever
/// over nothing but a decimal point.
#[test]
fn an_int_where_a_float_ships_is_not_a_departure() {
    let world = World::stowed("save-int-float");
    assert!(set_and_save(&world, &[(PreferenceKey::PointerSensitivity, "0")]).is_empty());
    assert_eq!(world.preferences_file().as_deref(), Some(FACTORY));
}

/// Sections come out in layer 1's own order, because the departures walk is driven by
/// layer 1 -- not in the order the keys happened to be set.
#[test]
fn several_departures_come_out_in_the_shipped_files_order() {
    let world = World::stowed("save-many");
    let notes = set_and_save(
        &world,
        &[
            (PreferenceKey::AccentColor, "\"red\""),
            (PreferenceKey::LockTimeout, "300"),
            (PreferenceKey::BarHeight, "50"),
            (PreferenceKey::NaturalScroll, "true"),
            (PreferenceKey::RegionTimeFormat, "\"12\""),
        ],
    );
    assert!(notes.is_empty());
    assert_eq!(
        world.preferences_file().as_deref(),
        Some(
            "[schema]\npreferences_version = 5\n\n\
             [appearance]\naccent_color = \"red\"\n\n\
             [bar]\nheight = 50\n\n\
             [input]\nnatural_scroll = true\n\n\
             [lock]\nlock_timeout = 300\n\n\
             [region]\ntime_format = \"12\"\n"
        )
    );
}

/// A save is a rename over the target, so the file a reader opens is either the whole of
/// the old one or the whole of the new one -- never the middle of this write.
#[test]
fn a_save_replaces_the_file_rather_than_editing_it() {
    let world = World::stowed("save-atomic");
    world.plant_preferences("# hand written\n");
    let config = Preferences::coerce_from(
        &toml::Table::new(),
        &crate::load::shipped_defaults(world.paths()).expect("layer 1 reads"),
        &mut Notes::new(),
    );
    let lock = PrefLock::acquire(world.paths()).expect("the lock is free");
    save_preferences(world.paths(), &config, &lock, None).expect("the save succeeds");
    assert_eq!(world.preferences_file().as_deref(), Some(FACTORY));
    assert_eq!(
        listing(&world.paths().root),
        ["preferences.defaults.toml", "preferences.toml"],
        "no temporary is left beside it"
    );
}

/// **Divergence, written down rather than hidden.** The Python's `config` is a dict, so a
/// load whose compaction was skipped can carry an unknown key straight into the save, which
/// is where it reports `"<key> is not a preference this build has; dropping it"` and drops
/// it. A [`Preferences`] cannot hold one, so that note is unreachable from here -- and the
/// file that lands is identical either way, because the departures walk is driven by layer
/// 1 and an unknown key could never have reached the output.
#[test]
fn an_unknown_key_cannot_reach_a_save_at_all() {
    let world = World::stowed("save-unknown");
    world.plant_preferences(
        "[schema]\npreferences_version = 5\n\n[appearance]\naccent_color = \"red\"\n\
         not_a_key = 1\n",
    );
    let mut sink = Vec::new();
    let config = load_preferences(world.paths(), Some(&mut sink)).expect("the load succeeds");
    let lock = PrefLock::acquire(world.paths()).expect("the lock is free");
    save_preferences(world.paths(), &config, &lock, Some(&mut sink)).expect("the save succeeds");
    assert!(
        sink.is_empty(),
        "the Python reports the dropped key here; nothing here can carry one"
    );
    assert_eq!(
        world.preferences_file().as_deref(),
        Some("[schema]\npreferences_version = 5\n\n[appearance]\naccent_color = \"red\"\n"),
        "the file is what the Python writes either way"
    );
}
