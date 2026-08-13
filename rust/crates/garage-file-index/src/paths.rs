//! [`IndexPaths`] -- the five filesystem locations `desktop/.local/bin/garage-file-index`
//! resolves from the environment at startup.
//!
//! Three of the five are already [`garage_core::paths::Paths`] fields: `HOME`, layer 1's
//! `preferences.defaults.toml`, and layer 2's `preferences.toml` (the `GARAGE_PREFERENCES`
//! override). Those are taken from there rather than re-derived, so a change to how the rest
//! of the desktop resolves `$XDG_CONFIG_HOME` is not a second place to update.
//!
//! The other two -- the database and its lock file -- live under `$XDG_CACHE_HOME`, not
//! `$XDG_STATE_HOME`, and `garage_core::paths::Paths` has no field for that base at all: the
//! rest of the desktop has no cache-scoped state. Adding one there for a single caller would
//! be scope creep on a struct another agent owns, so [`IndexPaths::cache_home`] is derived by
//! hand, the same way `garage-file-index` derives `CACHE_HOME` itself.

use std::collections::HashMap;
use std::path::PathBuf;

use garage_core::paths::Paths;

/// Every filesystem location this binary reads or writes.
#[derive(Debug, Clone)]
pub(crate) struct IndexPaths {
    /// `$HOME`, via [`garage_core::paths::Paths::home`].
    pub home: PathBuf,
    /// Layer 1: `$XDG_CONFIG_HOME/garage/preferences.defaults.toml`.
    pub defaults_path: PathBuf,
    /// Layer 2: `$GARAGE_PREFERENCES`, defaulting to
    /// `$XDG_CONFIG_HOME/garage/preferences.toml`.
    pub preferences_path: PathBuf,
    /// `$XDG_CACHE_HOME`, defaulting to `~/.cache`. Not a `garage_core::paths::Paths` field --
    /// see the module docs. Kept as a field (rather than folded straight into
    /// [`IndexPaths::database_path`] and [`IndexPaths::lock_path`]) so it stays inspectable
    /// on its own, the way the Python's module-level `CACHE_HOME` is; no production code
    /// reads it back out once the two derived paths below are built, only the tests that
    /// pin its resolution.
    #[allow(dead_code)]
    pub cache_home: PathBuf,
    /// `$GARAGE_FILE_INDEX_DB`, defaulting to `$XDG_CACHE_HOME/garage/file-index.sqlite3`.
    pub database_path: PathBuf,
    /// `$GARAGE_FILE_INDEX_LOCK`, defaulting to `$XDG_CACHE_HOME/garage/file-index.lock`.
    pub lock_path: PathBuf,
}

impl IndexPaths {
    /// Resolve every path from this process's environment.
    #[must_use]
    pub(crate) fn from_env() -> Self {
        Self::from_env_map(&std::env::vars().collect())
    }

    /// Resolve every path from an explicit environment, so a test can name a complete world
    /// without mutating the process's own.
    #[must_use]
    pub(crate) fn from_env_map(env: &HashMap<String, String>) -> Self {
        let core = Paths::from_env_map(env);
        let cache_home =
            value(env, "XDG_CACHE_HOME").map_or_else(|| core.home.join(".cache"), PathBuf::from);
        let cache_root = cache_home.join("garage");
        let database_path = value(env, "GARAGE_FILE_INDEX_DB")
            .map_or_else(|| cache_root.join("file-index.sqlite3"), PathBuf::from);
        let lock_path = value(env, "GARAGE_FILE_INDEX_LOCK")
            .map_or_else(|| cache_root.join("file-index.lock"), PathBuf::from);
        Self {
            home: core.home,
            defaults_path: core.defaults_path,
            preferences_path: core.host.preferences,
            cache_home,
            database_path,
            lock_path,
        }
    }
}

/// An environment variable, treating unset and empty alike -- matching
/// [`garage_core::paths`]'s own `value()`, which is private to that module.
fn value<'a>(env: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    env.get(key)
        .map(String::as_str)
        .filter(|found| !found.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{HashMap, IndexPaths, PathBuf};

    fn env_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, held)| ((*key).to_owned(), (*held).to_owned()))
            .collect()
    }

    #[test]
    fn defaults_fall_back_to_the_xdg_cache_home() {
        let paths = IndexPaths::from_env_map(&env_of(&[("HOME", "/home/tester")]));
        assert_eq!(paths.cache_home, PathBuf::from("/home/tester/.cache"));
        assert_eq!(
            paths.database_path,
            PathBuf::from("/home/tester/.cache/garage/file-index.sqlite3")
        );
        assert_eq!(
            paths.lock_path,
            PathBuf::from("/home/tester/.cache/garage/file-index.lock")
        );
        assert_eq!(
            paths.defaults_path,
            PathBuf::from("/home/tester/.config/garage/preferences.defaults.toml")
        );
        assert_eq!(
            paths.preferences_path,
            PathBuf::from("/home/tester/.config/garage/preferences.toml")
        );
    }

    #[test]
    fn every_override_is_honoured_independently() {
        let paths = IndexPaths::from_env_map(&env_of(&[
            ("HOME", "/home/tester"),
            ("XDG_CACHE_HOME", "/elsewhere/cache"),
            ("GARAGE_PREFERENCES", "/tmp/p.toml"),
            ("GARAGE_FILE_INDEX_DB", "/tmp/files.sqlite3"),
            ("GARAGE_FILE_INDEX_LOCK", "/tmp/files.lock"),
        ]));
        assert_eq!(paths.cache_home, PathBuf::from("/elsewhere/cache"));
        assert_eq!(paths.database_path, PathBuf::from("/tmp/files.sqlite3"));
        assert_eq!(paths.lock_path, PathBuf::from("/tmp/files.lock"));
        assert_eq!(paths.preferences_path, PathBuf::from("/tmp/p.toml"));
    }

    #[test]
    fn constructing_paths_creates_nothing() {
        let temp = std::env::temp_dir().join(format!(
            "garage-file-index-paths-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let home = temp.to_string_lossy().into_owned();
        let paths = IndexPaths::from_env_map(&env_of(&[("HOME", &home)]));
        assert!(!paths.cache_home.exists());
        assert!(!paths.database_path.exists());
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
