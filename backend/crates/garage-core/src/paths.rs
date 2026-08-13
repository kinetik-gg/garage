//! `Paths` — every filesystem location, built from the environment at startup and
//! threaded through. No statics: tests redirect the whole tree by constructing one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Every filesystem location the desktop reads or writes, resolved once from the
/// environment.
///
/// Three layers, in the order they win, and nothing writes to a layer above its own:
///
/// 1. Shipped defaults -- [`Paths::defaults_path`], read-only, reaching
///    `~/.config/garage` as a stow symlink into the dotfiles checkout. Every key the
///    schema has, with the value a fresh install starts on.
/// 2. Host preferences -- [`Paths::root`], the only user-owned files on the machine: the
///    four in [`HostFiles`], each holding the deliberate departures from layer 1 and
///    nothing else. Back these up and the desktop comes back.
/// 3. Generated state -- [`Paths::state_root`], machine-written and deletable. Every
///    fragment under it is rewritten from layers 1 and 2 by `garage render`, so losing
///    the whole directory costs one render and no settings.
///
/// Kept apart because they have different lifetimes: layer 1 moves with the checkout,
/// layer 2 must survive a reinstall, and layer 3 is a cache that has to be safe to throw
/// away. Generated output used to sit inside layer 2, where deleting the cache meant
/// deleting the user's settings beside it.
///
/// Building one is path arithmetic and nothing else. No directory is created, no file is
/// opened, and no environment variable is written back, so a process that only wants to
/// know where something lives leaves the disk exactly as it found it.
#[derive(Debug, Clone)]
pub struct Paths {
    /// `$HOME`.
    pub home: PathBuf,
    /// `$XDG_CONFIG_HOME`, defaulting to `~/.config`.
    pub config_home: PathBuf,
    /// `$XDG_STATE_HOME`, defaulting to `~/.local/state`.
    pub state_home: PathBuf,
    /// Layer 2's directory.
    pub root: PathBuf,
    /// Layer 3's directory.
    pub state_root: PathBuf,
    /// Where layer 3's fragments are written, under [`Paths::state_root`].
    pub generated: PathBuf,
    /// Layer 1: the shipped defaults, stowed read-only into [`Paths::root`].
    pub defaults_path: PathBuf,
    /// Layer 2's four files.
    pub host: HostFiles,
    /// Layer 3's rendered fragments.
    pub fragments: Fragments,
    /// Layer 3's watched marker files.
    pub markers: Markers,
    /// The two advisory locks, under [`Paths::state_root`].
    pub locks: Locks,
    /// A display layout that has been applied and is waiting to be confirmed or rolled
    /// back, under [`Paths::state_root`].
    pub pending_display: PathBuf,
    /// The wallpaper directory and the link the compositor reads.
    pub wallpaper: Wallpaper,
    /// The toolkit configs a theme switch has to rewrite in full.
    pub toolkit: Toolkit,
    /// Where the default applications are written. Not `~/.config/mimeapps.list`: that
    /// path is a stow symlink into the dotfiles repo, and every writer of it --
    /// `xdg-mime` included -- renames a temporary over it, which replaces the link with
    /// a plain file and cuts the config loose from the repo. The spec reads a
    /// `<desktop>`-prefixed list ahead of the plain one, so this file wins outright while
    /// the tracked one stays exactly as checked in.
    pub mimeapps_override: PathBuf,
    /// Where layer 2 was kept before the rename. The files that belong to the user rather
    /// than to the machine are migrated out of here into [`Paths::host`].
    pub legacy_root: PathBuf,
    /// Where `garage-rebuild-plugins` deploys, and the one system path in this struct.
    /// Duplicated from that script rather than parsed out of it: it is a published layout
    /// that the pacman hook also hardcodes, not a private detail.
    pub plugin_root: PathBuf,
    /// The XDG application directories, in lookup order: `$XDG_DATA_HOME/applications`
    /// first, then each of `$XDG_DATA_DIRS`'s `applications` subdirectories
    /// (`application_dirs()`, garage:4252-4256). `$XDG_DATA_HOME` defaults to
    /// `~/.local/share` and `$XDG_DATA_DIRS` to `/usr/local/share:/usr/share`, both read
    /// here rather than by each caller so a `.desktop` lookup agrees with every other
    /// XDG-aware tool on the system about where one can live.
    pub application_dirs: Vec<PathBuf>,
}

impl Paths {
    /// Resolve every path from this process's environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_env_map(&std::env::vars().collect())
    }

    /// Resolve every path from an explicit environment.
    ///
    /// The whole environment arrives as one argument so that a caller -- a test, or a
    /// subcommand rendering for somewhere other than this session -- can name a complete
    /// world without mutating the process's own, which is global and shared with every
    /// thread.
    #[must_use]
    pub fn from_env_map(env: &HashMap<String, String>) -> Self {
        let home = PathBuf::from(value(env, "HOME").unwrap_or("/"));
        let config_home = path_or(env, "XDG_CONFIG_HOME", home.join(".config"));
        let state_home = path_or(env, "XDG_STATE_HOME", home.join(".local/state"));
        let root = config_home.join("garage");
        let state_root = state_home.join("garage");
        let generated = state_root.join("generated");
        Self {
            defaults_path: root.join("preferences.defaults.toml"),
            host: HostFiles::new(env, &root),
            fragments: Fragments::new(&generated),
            markers: Markers::new(&generated),
            locks: Locks::new(&state_root),
            pending_display: state_root.join("display-pending.json"),
            wallpaper: Wallpaper::new(&home),
            toolkit: Toolkit::new(&config_home),
            mimeapps_override: config_home.join(mimeapps_name(env)),
            legacy_root: config_home.join("workstation"),
            plugin_root: PathBuf::from("/usr/lib/kinetik/plugins"),
            application_dirs: application_dirs(env, &home),
            home,
            config_home,
            state_home,
            root,
            state_root,
            generated,
        }
    }
}

/// Layer 2: the only user-owned files on the machine.
///
/// Each carries an environment override, so one render can be pointed at a different set
/// without the session's own files being touched.
#[derive(Debug, Clone)]
pub struct HostFiles {
    /// `$GARAGE_PREFERENCES`, defaulting to `preferences.toml` under the root.
    pub preferences: PathBuf,
    /// `$GARAGE_DISPLAYS`, defaulting to `displays.toml` under the root.
    pub displays: PathBuf,
    /// `$GARAGE_KEYBINDINGS`, defaulting to `keybindings.toml` under the root.
    ///
    /// A file of its own, next to `displays.toml` and for the same reason: the shortcuts
    /// are a list of records, and `preferences.toml` is a flat section/key whitelist that
    /// has nowhere to put one.
    pub keybindings: PathBuf,
    /// `$GARAGE_WORKSPACE_BLOCKS`, defaulting to `workspace-blocks.toml` under the root.
    ///
    /// Which block of workspace ids each display owns. Not in `displays.toml`, though it
    /// is per-display state: that file is rewritten from the monitors Hyprland can see
    /// whenever a layout is applied, so a display that happened to be asleep would lose
    /// its block and be handed somebody else's on the way back.
    pub workspace_blocks: PathBuf,
}

impl HostFiles {
    fn new(env: &HashMap<String, String>, root: &Path) -> Self {
        Self {
            preferences: path_or(env, "GARAGE_PREFERENCES", root.join("preferences.toml")),
            displays: path_or(env, "GARAGE_DISPLAYS", root.join("displays.toml")),
            keybindings: path_or(env, "GARAGE_KEYBINDINGS", root.join("keybindings.toml")),
            workspace_blocks: path_or(
                env,
                "GARAGE_WORKSPACE_BLOCKS",
                root.join("workspace-blocks.toml"),
            ),
        }
    }
}

/// Layer 3's rendered fragments: files a consumer parses, each one rewritten whole by
/// `garage render` and safe to delete.
#[derive(Debug, Clone)]
pub struct Fragments {
    /// The Hyprland options fragment, `dofile()`d from `hyprland.lua`.
    pub hyprland: PathBuf,
    /// The monitor rules fragment.
    pub displays: PathBuf,
    /// The workspace rules fragment.
    pub workspaces: PathBuf,
    /// The keybinds, as the shell reads them.
    pub keybinds_data: PathBuf,
    /// The keybinds, as Hyprland's parser reads them.
    pub keybinds_catalog: PathBuf,
    /// `hypridle`'s whole config, reread only when the unit restarts.
    pub hypridle: PathBuf,
    /// `hyprpaper`'s whole config, reread only when the unit restarts.
    pub hyprpaper: PathBuf,
    /// `LANG` and its neighbours, for what the session starts next.
    pub locale_env: PathBuf,
    /// The bar's clock module.
    pub waybar_clock: PathBuf,
    /// The bar's workspace module.
    pub waybar_workspaces: PathBuf,
    /// The bar's widget modules.
    pub waybar_widgets: PathBuf,
}

impl Fragments {
    fn new(generated: &Path) -> Self {
        Self {
            hyprland: generated.join("preferences.lua"),
            displays: generated.join("displays.lua"),
            workspaces: generated.join("workspaces.lua"),
            keybinds_data: generated.join("keybinds.json"),
            keybinds_catalog: generated.join("keybinds.catalog"),
            hypridle: generated.join("hypridle.conf"),
            hyprpaper: generated.join("hyprpaper.conf"),
            locale_env: generated.join("locale.env"),
            waybar_clock: generated.join("waybar-clock.jsonc"),
            waybar_workspaces: generated.join("waybar-workspaces.jsonc"),
            waybar_widgets: generated.join("waybar-widgets.jsonc"),
        }
    }
}

/// The files that are watched rather than read: Quickshell, and the wrappers the keybinds
/// run, hold an inotify watch on each of these. Nothing is signalled and nothing is
/// restarted; the write *is* the apply.
///
/// Every one of them must be written with [`crate::fs::marker::write_marker`] and never
/// with [`crate::fs::atomic::atomic_write`], because a rename past an inotify watch is
/// invisible to the watcher.
#[derive(Debug, Clone)]
pub struct Markers {
    /// The accent colour.
    pub accent: PathBuf,
    /// `light` or `dark`.
    pub color_scheme: PathBuf,
    /// The search engine the launcher hands a query to.
    pub search_engine: PathBuf,
    /// The bar's foreground colour.
    pub bar_foreground: PathBuf,
    /// The corner radius, in px.
    pub corner_radius: PathBuf,
    /// The Material palette the shell tints itself from.
    pub material: PathBuf,
    /// The launcher a keybind runs.
    pub launcher: PathBuf,
    /// The terminal a keybind runs.
    pub terminal: PathBuf,
    /// The browser a keybind runs.
    pub browser: PathBuf,
    /// Whether animations are suppressed.
    pub reduce_motion: PathBuf,
}

impl Markers {
    fn new(generated: &Path) -> Self {
        Self {
            accent: generated.join("accent"),
            color_scheme: generated.join("color-scheme"),
            search_engine: generated.join("search-engine"),
            bar_foreground: generated.join("bar-foreground"),
            corner_radius: generated.join("corner-radius"),
            material: generated.join("material"),
            launcher: generated.join("launcher"),
            terminal: generated.join("terminal"),
            browser: generated.join("browser"),
            reduce_motion: generated.join("reduce-motion"),
        }
    }
}

/// The advisory locks each read-modify-write is held across.
#[derive(Debug, Clone)]
pub struct Locks {
    /// Held across a display transaction: apply, then confirm or roll back.
    pub display: PathBuf,
    /// Held by every caller across its own read of layer 2 and the write that follows.
    pub preferences: PathBuf,
}

impl Locks {
    fn new(state_root: &Path) -> Self {
        Self {
            display: state_root.join("display-transaction.lock"),
            preferences: state_root.join("preferences.lock"),
        }
    }
}

/// Where the wallpaper is kept, outside all three layers: it is neither a preference nor
/// a rendered fragment, and re-rendering does not reproduce it.
#[derive(Debug, Clone)]
pub struct Wallpaper {
    /// The directory the processed wallpapers land in.
    pub directory: PathBuf,
    /// The one the compositor is pointed at.
    pub current: PathBuf,
}

impl Wallpaper {
    fn new(home: &Path) -> Self {
        let directory = home.join(".local/share/wallpaper");
        Self {
            current: directory.join("current"),
            directory,
        }
    }
}

/// Toolkit configs that offer no include or indirection mechanism, so a theme switch has
/// to rewrite the file itself. They are deliberately not stowed: every other path under
/// `~/.config` is a symlink into the dotfiles repo, and rewriting one of those at runtime
/// would edit a tracked file on every switch.
#[derive(Debug, Clone)]
pub struct Toolkit {
    /// The bar's stylesheet.
    pub waybar_style: PathBuf,
    /// GTK 3's settings.
    pub gtk3_settings: PathBuf,
    /// GTK 4's settings.
    pub gtk4_settings: PathBuf,
    /// What XSettings-speaking X11 clients read.
    pub xsettingsd: PathBuf,
    /// Qt 6's platform theme settings.
    pub qt6ct: PathBuf,
}

impl Toolkit {
    fn new(config_home: &Path) -> Self {
        Self {
            waybar_style: config_home.join("waybar/style.css"),
            gtk3_settings: config_home.join("gtk-3.0/settings.ini"),
            gtk4_settings: config_home.join("gtk-4.0/settings.ini"),
            xsettingsd: config_home.join("xsettingsd/xsettingsd.conf"),
            qt6ct: config_home.join("qt6ct/qt6ct.conf"),
        }
    }
}

/// An environment variable, treating unset and empty alike: an exported-but-empty
/// `XDG_CONFIG_HOME` names no directory, so it falls back like an absent one.
fn value<'a>(env: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    env.get(key)
        .map(String::as_str)
        .filter(|found| !found.is_empty())
}

fn path_or(env: &HashMap<String, String>, key: &str, fallback: PathBuf) -> PathBuf {
    value(env, key).map_or(fallback, PathBuf::from)
}

/// `application_dirs()` (garage:4252-4256): `$XDG_DATA_HOME/applications` first, then each
/// `:`-separated, non-empty entry of `$XDG_DATA_DIRS` with `applications` appended --
/// `$XDG_DATA_DIRS` itself falling back to `/usr/local/share:/usr/share` when unset or
/// empty, exactly as `os.environ.get("XDG_DATA_DIRS") or "..."` does.
fn application_dirs(env: &HashMap<String, String>, home: &Path) -> Vec<PathBuf> {
    let data_home = path_or(env, "XDG_DATA_HOME", home.join(".local/share"));
    let data_dirs = value(env, "XDG_DATA_DIRS").unwrap_or("/usr/local/share:/usr/share");
    let mut dirs = vec![data_home.join("applications")];
    dirs.extend(
        data_dirs
            .split(':')
            .filter(|item| !item.is_empty())
            .map(|item| Path::new(item).join("applications")),
    );
    dirs
}

/// The `<desktop>-mimeapps.list` name the spec reads ahead of the plain one: the first
/// segment of `XDG_CURRENT_DESKTOP`, lowercased, and `hyprland` when there is none.
fn mimeapps_name(env: &HashMap<String, String>) -> String {
    let desktop = value(env, "XDG_CURRENT_DESKTOP")
        .and_then(|current| current.split(':').next())
        .filter(|first| !first.is_empty())
        .unwrap_or("hyprland")
        .to_lowercase();
    format!("{desktop}-mimeapps.list")
}

#[cfg(test)]
mod tests {
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
}
