//! v4's file move, case by case, against the Python's own answers.

use std::fs;

use crate::config_root::migrate_config_root;
use crate::testing::{listing, World};

/// The four names layer 2 is made of, planted in the old config root.
const LEGACY: [(&str, &str); 4] = [
    ("preferences.toml", "[appearance]\naccent_color = \"red\"\n"),
    ("displays.toml", "# displays\n"),
    ("keybindings.toml", "# keys\n"),
    ("workspace-blocks.toml", "# blocks\n"),
];

fn migrate(world: &World) {
    migrate_config_root(world.paths()).expect("the migration succeeds");
}

#[test]
fn every_user_owned_file_moves_and_the_old_directory_goes() {
    let world = World::stowed("legacy-v1");
    for (name, text) in LEGACY {
        world.plant_legacy(name, text);
    }
    migrate(&world);
    assert!(
        !world.paths().legacy_root.exists(),
        "the old root is emptied and removed"
    );
    assert_eq!(
        listing(&world.paths().root),
        [
            "displays.toml",
            "keybindings.toml",
            "preferences.defaults.toml",
            "preferences.toml",
            "workspace-blocks.toml"
        ]
    );
    assert_eq!(
        world.preferences_file().as_deref(),
        Some("[appearance]\naccent_color = \"red\"\n"),
        "the file arrives byte for byte"
    );
}

/// Gated on the same stamp as every other step, and read from the *old* file because the
/// new path is still empty at this point.
#[test]
fn a_file_already_past_v4_is_left_where_it_is() {
    for stamp in [4, 5] {
        let world = World::stowed(&format!("legacy-v{stamp}"));
        world.plant_legacy(
            "preferences.toml",
            &format!("[schema]\npreferences_version = {stamp}\n"),
        );
        world.plant_legacy("displays.toml", "# displays\n");
        migrate(&world);
        assert_eq!(
            listing(&world.paths().legacy_root),
            ["displays.toml", "preferences.toml"],
            "stamp {stamp}"
        );
        assert_eq!(listing(&world.paths().root), ["preferences.defaults.toml"]);
    }
}

/// Unparseable is "older than current" rather than fatal: the file is still the user's, and
/// it is worth more at the new path -- where the loader can report it once -- than stranded
/// at the old one.
#[test]
fn an_unparseable_old_file_is_still_carried_across() {
    let world = World::stowed("legacy-unparseable");
    world.plant_legacy("preferences.toml", "not toml at all\n");
    world.plant_legacy("displays.toml", "# displays\n");
    migrate(&world);
    assert!(!world.paths().legacy_root.exists());
    assert_eq!(
        world.preferences_file().as_deref(),
        Some("not toml at all\n")
    );
}

/// Never over a file already at the new location. That one is what the session has been
/// reading, so the old one is the stale copy -- and it stays behind rather than being
/// deleted, which is why the directory survives too.
#[test]
fn a_file_already_at_the_new_path_wins() {
    let world = World::stowed("legacy-occupied");
    world.plant_legacy("preferences.toml", "# old\n");
    world.plant_legacy("displays.toml", "# displays\n");
    world.plant_preferences("# new, wins\n");
    migrate(&world);
    assert_eq!(world.preferences_file().as_deref(), Some("# new, wins\n"));
    assert_eq!(listing(&world.paths().legacy_root), ["preferences.toml"]);
    assert!(
        world.paths().host.displays.exists(),
        "the others still move"
    );
}

/// A symlink is layer 1 or a hand-made link, not the user's file: following it would drag a
/// tracked file out of the dotfiles checkout.
#[test]
fn a_symlink_is_never_followed_and_never_moved() {
    let world = World::stowed("legacy-symlink");
    world.plant_legacy("preferences.toml", "# old\n");
    let link = world.paths().legacy_root.join("displays.toml");
    std::os::unix::fs::symlink("/nowhere", &link).expect("the link is creatable");
    migrate(&world);
    assert_eq!(world.preferences_file().as_deref(), Some("# old\n"));
    assert_eq!(
        listing(&world.paths().legacy_root),
        ["displays.toml@"],
        "the link stays, and it keeps the directory alive"
    );
}

/// `generated/` is layer 3, machine-written, and `garage render` rewrites it under the
/// state root anyway. Anything left in the old directory means the removal refuses, which
/// is exactly the wanted outcome.
#[test]
fn a_leftover_directory_keeps_the_old_root_alive() {
    let world = World::stowed("legacy-generated");
    world.plant_legacy("preferences.toml", "# old\n");
    world.plant_legacy("generated/accent", "#fff\n");
    migrate(&world);
    assert_eq!(listing(&world.paths().legacy_root), ["generated"]);
    assert_eq!(world.preferences_file().as_deref(), Some("# old\n"));
}

#[test]
fn an_old_root_that_is_a_file_or_absent_is_a_no_op() {
    let world = World::stowed("legacy-absent");
    migrate(&world);
    assert!(!world.paths().legacy_root.exists());

    let world = World::stowed("legacy-file");
    fs::write(&world.paths().legacy_root, "not a directory\n").expect("writable");
    migrate(&world);
    assert!(world.paths().legacy_root.is_file(), "left exactly as it is");
}

/// Idempotent by construction: a second run finds nothing left to move.
#[test]
fn running_it_twice_changes_nothing() {
    let world = World::stowed("legacy-twice");
    for (name, text) in LEGACY {
        world.plant_legacy(name, text);
    }
    migrate(&world);
    let after = listing(&world.paths().root);
    migrate(&world);
    assert_eq!(listing(&world.paths().root), after);
}

/// An env-overridden path is not the host config root: a second profile or a test harness
/// pointing elsewhere must not reach in and move the real session's files out from under it.
#[test]
fn an_overridden_target_outside_the_config_root_is_left_alone() {
    let world = World::stowed("legacy-overridden");
    for (name, text) in LEGACY {
        world.plant_legacy(name, text);
    }
    let mut env = std::collections::HashMap::new();
    env.insert(
        "HOME".to_owned(),
        world.home().to_string_lossy().into_owned(),
    );
    env.insert(
        "GARAGE_PREFERENCES".to_owned(),
        world
            .home()
            .join("elsewhere.toml")
            .to_string_lossy()
            .into_owned(),
    );
    let paths = garage_core::paths::Paths::from_env_map(&env);
    migrate_config_root(&paths).expect("the migration succeeds");
    assert!(
        !paths.host.preferences.exists(),
        "the overridden file is not written"
    );
    assert_eq!(
        listing(&paths.legacy_root),
        ["preferences.toml"],
        "and its source stays where it was, while the other three move"
    );
}
