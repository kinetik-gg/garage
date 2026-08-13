//! [`configuration`] -- the `[indexing]` section, layered from the two preference files and
//! clamped to a value the scanner can always use.
//!
//! Two files are read in a fixed order, defaults then host: `DEFAULTS_PATH`'s `[indexing]`
//! table is applied over the hardcoded fallback, then `PREFERENCES_PATH`'s is applied over
//! *that* -- key by key, so a file that sets only `enabled` leaves the other three keys
//! whatever the previous layer had. A key whose stored value is the wrong shape --
//! `frequency_minutes` out of `1..=1440`, `enabled` not a bool -- does not fall back to the
//! previous layer's value; it falls all the way back to the hardcoded default, exactly as
//! the Python's `configuration()` does by re-checking the merged dict's shape rather than
//! validating each layer as it lands.
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::paths::IndexPaths;

/// The roots indexed when nothing in either preferences file names any, one per line --
/// ported verbatim from the Python's `"\n".join(...)`.
pub(crate) const DEFAULT_DIRECTORIES: &str = "~/Desktop\n~/Documents\n~/Downloads\n~/Music\n~/Pictures\n~/Projects\n~/repositories\n~/Videos";

/// The resolved `[indexing]` section: what [`crate::refresh`], [`crate::search`] and
/// [`crate::status`] all act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Config {
    pub enabled: bool,
    pub frequency_minutes: i64,
    pub max_depth: i64,
    /// Every configured root, expanded, resolved, confined to `$HOME`, deduplicated, and
    /// reduced to only the broadest ones -- see [`configured_roots`].
    pub directories: Vec<PathBuf>,
}

/// Read the `[indexing]` section from both files and resolve it to a usable [`Config`].
///
/// # Errors
///
/// An [`io::Error`] if either file exists but cannot be read for a reason other than
/// "not found" or "permission denied" -- those two are swallowed, matching the Python's
/// `except (FileNotFoundError, PermissionError, tomllib.TOMLDecodeError)`, which a malformed
/// file also falls into (a parse failure is likewise swallowed here, never surfaced as an
/// error).
pub(crate) fn configuration(paths: &IndexPaths) -> Result<Config, io::Error> {
    let mut section = fallback_section();
    for candidate in [&paths.defaults_path, &paths.preferences_path] {
        let table = load_toml(candidate)?;
        if let Some(toml::Value::Table(values)) = table.get("indexing") {
            for (key, value) in values {
                section.insert(key.clone(), value.clone());
            }
        }
    }
    let enabled = section
        .get("enabled")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true);
    let frequency_minutes = section
        .get("frequency_minutes")
        .and_then(toml::Value::as_integer)
        .filter(|value| (1..=1440).contains(value))
        .unwrap_or(5);
    let max_depth = section
        .get("max_depth")
        .and_then(toml::Value::as_integer)
        .filter(|value| (1..=64).contains(value))
        .unwrap_or(8);
    let directories_value = section.get("directories").and_then(toml::Value::as_str);
    let home_resolved = resolve_lenient(&paths.home);
    let directories = configured_roots(directories_value, &paths.home, &home_resolved);
    Ok(Config {
        enabled,
        frequency_minutes,
        max_depth,
        directories,
    })
}

/// The hardcoded fallback, seeded as `toml::Value`s so it merges with a loaded `[indexing]`
/// table through the same map -- the Python's `FALLBACK` dict, ported.
fn fallback_section() -> HashMap<String, toml::Value> {
    let mut map = HashMap::new();
    map.insert("enabled".to_string(), toml::Value::Boolean(true));
    map.insert("frequency_minutes".to_string(), toml::Value::Integer(5));
    map.insert("max_depth".to_string(), toml::Value::Integer(8));
    map.insert(
        "directories".to_string(),
        toml::Value::String(DEFAULT_DIRECTORIES.to_string()),
    );
    map
}

/// Read one TOML file, treating a missing or unreadable file, and a malformed one, all as
/// "nothing configured here" -- the Python's `load_toml`.
fn load_toml(path: &Path) -> Result<toml::Table, io::Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text.parse::<toml::Table>().unwrap_or_default()),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            ) =>
        {
            Ok(toml::Table::new())
        }
        Err(error) => Err(error),
    }
}

/// Turn the `directories` preference into the list of roots the scanner walks: one per
/// non-blank line, `~`-expanded, made absolute under `$HOME`, resolved, confined to `$HOME`,
/// deduplicated by their resolved form, and finally reduced to only the broadest ones --
/// dropping a configured root that another configured root already contains, so `~` and
/// `~/Documents` both configured does not walk `~/Documents` twice.
fn configured_roots(value: Option<&str>, home: &Path, home_resolved: &Path) -> Vec<PathBuf> {
    let text = value.unwrap_or(DEFAULT_DIRECTORIES);
    let mut roots = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // `str.splitlines()` in the Python; the blank artifact an unpaired `\r`/`\n` split can
    // leave behind is trimmed away below like every other blank line, so a plain split on
    // both characters reproduces the visible behaviour without a dedicated line-ending state
    // machine.
    for raw in text.split(['\n', '\r']) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let expanded = expand_tilde(trimmed, home);
        let candidate = if expanded.is_absolute() {
            expanded
        } else {
            home.join(expanded)
        };
        let resolved = resolve_lenient(&candidate);
        if !under_home(&resolved, home_resolved) {
            continue;
        }
        let key = resolved.to_string_lossy().into_owned();
        if !seen.insert(key) {
            continue;
        }
        roots.push(resolved);
    }
    roots
        .iter()
        .filter(|candidate| {
            !roots
                .iter()
                .any(|parent| parent != *candidate && candidate.starts_with(parent))
        })
        .cloned()
        .collect()
}

/// `os.path.expanduser` for the two forms every configured root actually uses: a bare `~`
/// and `~/rest`. `~otheruser` is left unexpanded -- a deliberate, narrow gap from the
/// Python's full behaviour, which also resolves other accounts' home directories through the
/// system's user database; nothing in `preferences.defaults.toml` or a hand-edited
/// `preferences.toml` is expected to name one.
fn expand_tilde(text: &str, home: &Path) -> PathBuf {
    if text == "~" {
        home.to_path_buf()
    } else if let Some(rest) = text.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(text)
    }
}

/// Whether `path` (already resolved) sits at or under `home_resolved` -- the Python's
/// `under_home`, which relies on `Path.relative_to` succeeding.
fn under_home(path: &Path, home_resolved: &Path) -> bool {
    path.starts_with(home_resolved)
}

/// `Path.resolve(strict=False)`: normalise `.`/`..` and follow symlinks for every path
/// component that exists, leaving the rest joined literally once a component stops
/// existing -- `os.path.realpath`'s algorithm, which is what `pathlib.Path.resolve()`
/// delegates to. Takes an absolute path; every caller in this crate builds one before
/// calling in, matching the Python, which only ever resolves `HOME` (already absolute) or a
/// candidate already joined onto `HOME`.
fn resolve_lenient(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => resolved = PathBuf::from("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(part) => {
                resolved.push(part);
                follow_symlink_chain(&mut resolved);
            }
        }
    }
    resolved
}

/// Replace `resolved` with what it points to, repeatedly, for as long as it names a
/// symlink -- bounded at 40 hops, standing in for the kernel's own `ELOOP` limit rather
/// than looping forever on a cycle.
fn follow_symlink_chain(resolved: &mut PathBuf) {
    for _hop in 0..40 {
        let Ok(target) = fs::read_link(&resolved) else {
            return;
        };
        if target.is_absolute() {
            *resolved = target;
        } else {
            resolved.pop();
            resolved.push(target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{configured_roots, resolve_lenient, under_home};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(label: &str) -> Self {
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "garage-file-index-config-{label}-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.path));
        }
    }

    /// Mirrors the Python's `test_configured_roots_are_deduplicated_and_confined_to_home`.
    #[test]
    fn roots_are_deduplicated_and_confined_to_home() {
        let scratch = Scratch::new("dedup");
        let home = scratch.path();
        let inside = home.join("Documents");
        fs::create_dir_all(&inside).unwrap();
        let home_resolved = resolve_lenient(home);

        let text = format!(
            "{}\n{}\n/etc\n../outside\n",
            inside.display(),
            inside.display()
        );
        let roots = configured_roots(Some(&text), home, &home_resolved);
        assert_eq!(roots, vec![resolve_lenient(&inside)]);

        let text = format!(
            "{}\n{}\n{}\n",
            inside.display(),
            home.display(),
            inside.join("Projects").display()
        );
        let roots = configured_roots(Some(&text), home, &home_resolved);
        assert_eq!(roots, vec![home_resolved.clone()]);
    }

    #[test]
    fn a_missing_value_falls_back_to_the_default_directories() {
        let scratch = Scratch::new("default");
        let home = scratch.path();
        let home_resolved = resolve_lenient(home);
        let roots = configured_roots(None, home, &home_resolved);
        assert!(roots.contains(&resolve_lenient(&home.join("Documents"))));
        assert!(roots.contains(&resolve_lenient(&home.join("Downloads"))));
    }

    #[test]
    fn blank_lines_and_whitespace_are_ignored() {
        let scratch = Scratch::new("blank");
        let home = scratch.path();
        let inside = home.join("Documents");
        fs::create_dir_all(&inside).unwrap();
        let home_resolved = resolve_lenient(home);
        let text = format!("\n   \n  {}  \n\n", inside.display());
        let roots = configured_roots(Some(&text), home, &home_resolved);
        assert_eq!(roots, vec![resolve_lenient(&inside)]);
    }

    #[test]
    fn under_home_accepts_home_itself_and_its_descendants() {
        let home = Path::new("/home/tester");
        assert!(under_home(home, home));
        assert!(under_home(&home.join("Documents"), home));
        assert!(!under_home(Path::new("/etc"), home));
        assert!(!under_home(Path::new("/home/tester-other"), home));
    }

    #[test]
    fn resolve_lenient_normalises_dot_and_dot_dot_lexically() {
        let scratch = Scratch::new("resolve");
        let base = scratch.path().join("a/b");
        fs::create_dir_all(&base).unwrap();
        let messy = scratch.path().join("a/./b/../b/c");
        assert_eq!(resolve_lenient(&messy), resolve_lenient(&base).join("c"));
    }

    #[test]
    fn resolve_lenient_follows_a_symlinked_directory() {
        let scratch = Scratch::new("symlink");
        let real = scratch.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let link = scratch.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(
            resolve_lenient(&link.join("child")),
            resolve_lenient(&real).join("child")
        );
    }
}
