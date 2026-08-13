//! A scratch `$HOME` per test, and the shipped defaults planted in it.
//!
//! Every test in this crate touches the filesystem, because every function in it does. The
//! world is built the way the differential harness builds one -- a `HOME` of its own, with
//! `preferences.defaults.toml` at the path a stowed machine keeps it -- so that a test and a
//! parity run are asking the same question of the same layout.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use garage_core::paths::Paths;

/// `preferences.defaults.toml`, from the checkout this crate is built in. The same file
/// `Defaults::compiled()` reads, planted into the scratch so the runtime path is exercised
/// rather than only the fallback.
pub(crate) const DEFAULTS: &str =
    include_str!("../../../../desktop/.config/garage/preferences.defaults.toml");

static SERIAL: AtomicU64 = AtomicU64::new(0);

/// A scratch `$HOME` that removes itself, and the [`Paths`] resolved inside it.
#[derive(Debug)]
pub(crate) struct World {
    home: PathBuf,
    paths: Paths,
}

impl World {
    /// A fresh world with the shipped defaults in place and no `preferences.toml`.
    pub(crate) fn new(label: &str) -> Self {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let home = std::env::temp_dir().join(format!(
            "garage-prefs-{label}-{}-{serial}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&home));
        fs::create_dir_all(home.join(".config/garage")).expect("scratch is creatable");
        fs::create_dir_all(home.join(".local/state/garage")).expect("scratch is creatable");
        let mut env = HashMap::new();
        env.insert("HOME".to_owned(), home.to_string_lossy().into_owned());
        let paths = Paths::from_env_map(&env);
        Self { home, paths }
    }

    /// The same world with layer 1 planted, which is what a stowed machine has.
    pub(crate) fn stowed(label: &str) -> Self {
        let world = Self::new(label);
        world.plant_defaults(DEFAULTS);
        world
    }

    pub(crate) fn paths(&self) -> &Paths {
        &self.paths
    }

    pub(crate) fn home(&self) -> &Path {
        &self.home
    }

    pub(crate) fn plant_defaults(&self, text: &str) {
        write(&self.paths.defaults_path, text);
    }

    pub(crate) fn plant_preferences(&self, text: &str) {
        write(&self.paths.host.preferences, text);
    }

    /// `preferences.toml` as it stands on disk, byte for byte, or `None` if it is not there.
    pub(crate) fn preferences_file(&self) -> Option<String> {
        fs::read_to_string(&self.paths.host.preferences).ok()
    }

    /// A file under the old config root, for the `migrate_config_root` tests.
    pub(crate) fn plant_legacy(&self, name: &str, text: &str) {
        write(&self.paths.legacy_root.join(name), text);
    }
}

/// The names under a directory, sorted, with `@` marking a symlink -- enough to say what a
/// migration moved without depending on the order a readdir happens to give.
pub(crate) fn listing(directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            let link = entry
                .path()
                .symlink_metadata()
                .is_ok_and(|meta| meta.file_type().is_symlink());
            let suffix = if link { "@" } else { "" };
            format!("{}{suffix}", entry.file_name().to_string_lossy())
        })
        .collect();
    names.sort();
    names
}

impl Drop for World {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.home));
    }
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("scratch directory is creatable");
    }
    fs::write(path, text).expect("scratch file is writable");
}
