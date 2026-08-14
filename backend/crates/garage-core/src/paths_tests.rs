//! Tests for [`Paths`] and its environment-derived filesystem layout.
//!
//! They live beside rather than inside `paths.rs` to keep that production module below the
//! 500-line cap enforced by `tests/test_lint.py::FileShape`.

use super::{HashMap, PathBuf, Paths};
use crate::fs::scratch::Scratch;

fn env_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, held)| ((*key).to_owned(), (*held).to_owned()))
        .collect()
}

#[test]
fn defaults_fall_back_to_the_xdg_base_directories() {
    let paths = Paths::from_env_map(&env_of(&[("HOME", "/home/tester")]));
    assert_eq!(paths.root, PathBuf::from("/home/tester/.config/garage"));
    assert_eq!(
        paths.state_root,
        PathBuf::from("/home/tester/.local/state/garage")
    );
    assert_eq!(
        paths.generated,
        PathBuf::from("/home/tester/.local/state/garage/generated")
    );
    assert_eq!(
        paths.host.preferences,
        PathBuf::from("/home/tester/.config/garage/preferences.toml")
    );
    assert_eq!(
        paths.mimeapps_override,
        PathBuf::from("/home/tester/.config/hyprland-mimeapps.list")
    );
    assert_eq!(paths.plugin_root, PathBuf::from("/usr/lib/kinetik/plugins"));
}

#[test]
fn xdg_overrides_move_both_roots() {
    let paths = Paths::from_env_map(&env_of(&[
        ("HOME", "/home/tester"),
        ("XDG_CONFIG_HOME", "/elsewhere/config"),
        ("XDG_STATE_HOME", "/elsewhere/state"),
        ("XDG_CURRENT_DESKTOP", "Hyprland:wlroots"),
    ]));
    assert_eq!(paths.root, PathBuf::from("/elsewhere/config/garage"));
    assert_eq!(paths.state_root, PathBuf::from("/elsewhere/state/garage"));
    assert_eq!(
        paths.markers.accent,
        PathBuf::from("/elsewhere/state/garage/generated/accent")
    );
    assert_eq!(
        paths.toolkit.gtk4_settings,
        PathBuf::from("/elsewhere/config/gtk-4.0/settings.ini")
    );
    assert_eq!(
        paths.mimeapps_override,
        PathBuf::from("/elsewhere/config/hyprland-mimeapps.list")
    );
    assert_eq!(
        paths.legacy_root,
        PathBuf::from("/elsewhere/config/workstation")
    );
}

#[test]
fn every_garage_override_is_honoured() {
    let paths = Paths::from_env_map(&env_of(&[
        ("HOME", "/home/tester"),
        ("GARAGE_PREFERENCES", "/tmp/p.toml"),
        ("GARAGE_DISPLAYS", "/tmp/d.toml"),
        ("GARAGE_KEYBINDINGS", "/tmp/k.toml"),
        ("GARAGE_WORKSPACE_BLOCKS", "/tmp/w.toml"),
    ]));
    assert_eq!(paths.host.preferences, PathBuf::from("/tmp/p.toml"));
    assert_eq!(paths.host.displays, PathBuf::from("/tmp/d.toml"));
    assert_eq!(paths.host.keybindings, PathBuf::from("/tmp/k.toml"));
    assert_eq!(paths.host.workspace_blocks, PathBuf::from("/tmp/w.toml"));
    assert_eq!(
        paths.defaults_path,
        PathBuf::from("/home/tester/.config/garage/preferences.defaults.toml")
    );
}

#[test]
fn application_dirs_defaults_to_the_xdg_fallback_list() {
    let paths = Paths::from_env_map(&env_of(&[("HOME", "/home/tester")]));
    assert_eq!(
        paths.application_dirs,
        vec![
            PathBuf::from("/home/tester/.local/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            PathBuf::from("/usr/share/applications"),
        ]
    );
}

#[test]
fn application_dirs_honours_the_xdg_overrides_and_drops_empty_segments() {
    let paths = Paths::from_env_map(&env_of(&[
        ("HOME", "/home/tester"),
        ("XDG_DATA_HOME", "/elsewhere/data"),
        ("XDG_DATA_DIRS", "/a/share::/b/share:"),
    ]));
    assert_eq!(
        paths.application_dirs,
        vec![
            PathBuf::from("/elsewhere/data/applications"),
            PathBuf::from("/a/share/applications"),
            PathBuf::from("/b/share/applications"),
        ]
    );
}

#[test]
fn constructing_paths_creates_nothing() {
    let scratch = Scratch::new("paths");
    let home = scratch.path().to_string_lossy().into_owned();
    let paths = Paths::from_env_map(&env_of(&[("HOME", &home)]));
    assert!(!paths.root.exists());
    assert!(!paths.state_root.exists());
    assert!(!paths.wallpaper.directory.exists());
    let left = std::fs::read_dir(scratch.path())
        .expect("scratch is readable")
        .count();
    assert_eq!(left, 0);
}
