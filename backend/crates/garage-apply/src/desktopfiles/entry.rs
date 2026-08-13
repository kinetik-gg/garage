//! `application_dirs()`, `desktop_file()` and `desktop_fields()`: finding and parsing a
//! `.desktop` file.
//!
//! `application_dirs()` follows the XDG lookup order -- `$XDG_DATA_HOME/applications` first,
//! then each of `$XDG_DATA_DIRS`'s `applications` subdirectories -- and `desktop_file()`
//! walks that list in order and returns the first match, which is what makes the resolution
//! agree with what every other XDG-aware tool on the system would resolve to.
//!
//! `desktop_fields()` reads `[Desktop Entry]`'s keys by hand rather than through a general
//! `.ini` reader: an `Exec` value legitimately carries `%` and `;`, which a reader that does
//! percent-interpolation would choke on, and a desktop file legitimately repeats a key in
//! localised forms (`Name[fr]`, `Name[de]`, ...) that some `.ini` readers reject outright as
//! duplicates. Only the first value seen for any given key is kept, which for the
//! unlocalised keys this reads is simply "first line wins".
//!
//! Returns paths and field maps, not `Result<(), ApplyError>`: none of the three is a
//! [`Route`](garage_core::schema::routes::Route) step, so nothing here has an apply-shaped
//! failure to report -- an unreadable or missing file is simply an empty answer, as it is in
//! the Python.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;

/// The XDG application directories, in lookup order (garage:4252-4256).
///
/// [`Paths::application_dirs`] already resolves `$XDG_DATA_HOME` and `$XDG_DATA_DIRS`
/// exactly the way every other XDG-aware tool on the system does -- see its own doc -- so
/// this is a thin pass-through, kept only so this module carries the function the Python
/// names alongside [`desktop_file`] and [`crate::desktopfiles::roles::terminal_candidates`],
/// which both walk the same list.
pub(crate) fn application_dirs(paths: &Paths) -> &[PathBuf] {
    &paths.application_dirs
}

/// Where a desktop id resolves in XDG lookup order, or `None` if nowhere (garage:4259-4265).
pub(crate) fn desktop_file(paths: &Paths, desktop_id: &str) -> Option<PathBuf> {
    application_dirs(paths).iter().find_map(|directory| {
        let candidate = directory.join(desktop_id);
        candidate.is_file().then_some(candidate)
    })
}

/// Every `*.desktop` name across every application directory -- `{path.name for directory
/// in application_dirs() if directory.is_dir() for path in directory.glob("*.desktop")}`,
/// used by [`crate::desktopfiles::roles::terminal_candidates`] to search by freedesktop
/// category rather than by mimetype. `Path.glob` skips names starting with `.`, matched here
/// even though nothing under an `applications` directory is legitimately dotted, so the
/// resolution agrees with the Python's by construction rather than by accident.
pub(crate) fn desktop_file_names(paths: &Paths) -> Vec<String> {
    let mut names = Vec::new();
    for directory in application_dirs(paths) {
        let Ok(read_dir) = std::fs::read_dir(directory) else {
            continue;
        };
        for found in read_dir.flatten() {
            let name = found.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') && name.ends_with(".desktop") {
                names.push(name);
            }
        }
    }
    names
}

/// The `[Desktop Entry]` keys of a desktop file, or an empty map for one that resolves
/// nowhere or cannot be read (garage:4268-4287).
///
/// Two small departures from the Python, both already established elsewhere in this crate:
///
/// * `.splitlines()` also breaks on `\v`, `\f`, `\x1c`-`\x1e`, `\x85` and the two Unicode
///   line separators; this splits on `\n`/`\r\n` alone, the same trade
///   [`crate::keybind::catalog::read_keybind_catalog`] makes for the same reason -- nothing
///   a desktop file legitimately carries writes one of those bytes into a value.
/// * `errors="replace"` on the Python's `read_text()` substitutes `U+FFFD` for a byte that
///   is not valid UTF-8 rather than failing; [`String::from_utf8_lossy`] does the same
///   substitution over the whole file at once.
pub(crate) fn desktop_fields(paths: &Paths, desktop_id: &str) -> HashMap<String, String> {
    let Some(path) = desktop_file(paths, desktop_id) else {
        return HashMap::new();
    };
    read_desktop_entry_section(&path)
}

/// The `[Desktop Entry]` section of an already-resolved desktop file.
fn read_desktop_entry_section(path: &Path) -> HashMap<String, String> {
    let Ok(bytes) = std::fs::read(path) else {
        return HashMap::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut fields = HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let stripped = line.trim();
        if stripped.starts_with('[') {
            stripped.clone_into(&mut section);
            continue;
        }
        if section != "[Desktop Entry]" || stripped.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = stripped.split_once('=') {
            fields
                .entry(key.trim().to_owned())
                .or_insert_with(|| value.trim().to_owned());
        }
    }
    fields
}

/// Shared test scaffolding for every submodule under `desktopfiles`: a self-cleaning
/// scratch directory that knows how to build a [`Paths`] rooted at itself and plant a
/// desktop file under it, plus the [`MonitorSource`]/[`LuaSyntaxCheck`]/[`SessionCx`] stubs
/// every render context needs. Centralised here, the base every other submodule already
/// depends on for its production code, rather than copied three times -- the same
/// one-copy-not-three reasoning [`crate::workspaces::run`] applies on the apply side.
#[cfg(test)]
pub(in crate::desktopfiles) mod support {
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::{
        LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource, Output, RunError,
        Runner,
    };
    use garage_render::cx::RenderCx;

    use crate::cx::SessionCx;

    /// A directory of its own per test, removed on drop.
    pub(in crate::desktopfiles) struct Scratch(PathBuf);

    impl Scratch {
        pub(in crate::desktopfiles) fn new(label: &str) -> Self {
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "garage-apply-desktopfiles-{label}-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("scratch directory is creatable");
            Self(path)
        }

        pub(in crate::desktopfiles) fn path(&self) -> &Path {
            &self.0
        }

        pub(in crate::desktopfiles) fn paths(&self) -> Paths {
            self.paths_with(&[])
        }

        pub(in crate::desktopfiles) fn paths_with(&self, extra: &[(&str, &str)]) -> Paths {
            let mut env: HashMap<String, String> =
                [("HOME".to_owned(), self.0.to_string_lossy().into_owned())]
                    .into_iter()
                    .collect();
            for (key, value) in extra {
                env.insert((*key).to_owned(), (*value).to_owned());
            }
            Paths::from_env_map(&env)
        }

        /// Not a method: nothing here reads from `self`, only from the `paths` a caller
        /// already built, possibly with [`Scratch::paths_with`] rather than [`Scratch::paths`].
        pub(in crate::desktopfiles) fn plant(paths: &Paths, desktop_id: &str, contents: &str) {
            let directory = paths.application_dirs.first().expect("data home is first");
            fs::create_dir_all(directory).expect("applications dir");
            fs::write(directory.join(desktop_id), contents).expect("write desktop file");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.0));
        }
    }

    pub(in crate::desktopfiles) struct NoMonitors;
    impl MonitorSource for NoMonitors {
        fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
            Ok(vec![])
        }
    }

    pub(in crate::desktopfiles) struct LuaAccepts;
    impl LuaSyntaxCheck for LuaAccepts {
        fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
            Ok(())
        }
    }

    /// A [`Runner`] whose `run()` is one closure and whose `spawn_detached()`/
    /// `run_streamed()` always succeed trivially -- neither is exercised by anything
    /// stubbed under `desktopfiles`. Every fake process in this crate's tests differs only
    /// in what `run()` answers, so this is the one `Runner` body the three submodules
    /// share instead of writing their own three-method `impl` apiece.
    pub(in crate::desktopfiles) struct RunOnly<F>(pub(in crate::desktopfiles) F);

    impl<F: Fn(&[&str]) -> Output> Runner for RunOnly<F> {
        fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
            Ok((self.0)(command))
        }

        fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
            Ok(())
        }

        fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
            Ok(0)
        }
    }

    /// A session context borrowing everything a test needs, so a test body is just the
    /// fixture it plants and the assertion it makes.
    pub(in crate::desktopfiles) fn session<'a>(
        paths: &'a Paths,
        defaults: &'a Defaults,
        proc: &'a dyn Runner,
    ) -> SessionCx<'a> {
        static MONITORS: NoMonitors = NoMonitors;
        static LUA: LuaAccepts = LuaAccepts;
        SessionCx::new(
            RenderCx::new(defaults.values(), paths, &MONITORS, &LUA),
            proc,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::support::Scratch;
    use super::{application_dirs, desktop_fields, desktop_file};

    #[test]
    fn application_dirs_is_the_paths_field_verbatim() {
        let scratch = Scratch::new("application-dirs");
        let paths = scratch.paths();
        assert_eq!(application_dirs(&paths), paths.application_dirs.as_slice());
    }

    #[test]
    fn desktop_file_prefers_data_home_over_data_dirs() {
        let scratch = Scratch::new("lookup-order");
        let data_home = scratch.path().join("home-apps");
        let data_dir = scratch.path().join("dirs-apps");
        std::fs::create_dir_all(data_home.join("applications")).expect("data home dir");
        std::fs::create_dir_all(data_dir.join("applications")).expect("data dir");
        std::fs::write(
            data_home.join("applications/kitty.desktop"),
            "[Desktop Entry]\nName=Kitty (home)\n",
        )
        .expect("write home entry");
        std::fs::write(
            data_dir.join("applications/kitty.desktop"),
            "[Desktop Entry]\nName=Kitty (dirs)\n",
        )
        .expect("write dirs entry");

        let paths = scratch.paths_with(&[
            ("XDG_DATA_HOME", &data_home.to_string_lossy()),
            ("XDG_DATA_DIRS", &data_dir.to_string_lossy()),
        ]);

        let found = desktop_file(&paths, "kitty.desktop").expect("found in data home");
        assert_eq!(found, data_home.join("applications/kitty.desktop"));
        assert_eq!(
            desktop_fields(&paths, "kitty.desktop").get("Name"),
            Some(&"Kitty (home)".to_owned())
        );
    }

    #[test]
    fn desktop_file_falls_back_to_data_dirs_when_absent_from_data_home() {
        let scratch = Scratch::new("fallback");
        let data_home = scratch.path().join("home-apps");
        let data_dir = scratch.path().join("dirs-apps");
        std::fs::create_dir_all(data_home.join("applications")).expect("data home dir");
        std::fs::create_dir_all(data_dir.join("applications")).expect("data dir");
        std::fs::write(
            data_dir.join("applications/firefox.desktop"),
            "[Desktop Entry]\nName=Firefox\n",
        )
        .expect("write dirs entry");

        let paths = scratch.paths_with(&[
            ("XDG_DATA_HOME", &data_home.to_string_lossy()),
            ("XDG_DATA_DIRS", &data_dir.to_string_lossy()),
        ]);

        assert_eq!(
            desktop_file(&paths, "firefox.desktop"),
            Some(data_dir.join("applications/firefox.desktop"))
        );
    }

    #[test]
    fn desktop_file_resolves_to_none_when_nowhere_on_the_lookup_path() {
        let scratch = Scratch::new("missing");
        let paths = scratch.paths();
        assert_eq!(desktop_file(&paths, "nowhere.desktop"), None);
        assert_eq!(
            desktop_fields(&paths, "nowhere.desktop"),
            std::collections::HashMap::new()
        );
    }

    #[test]
    fn desktop_fields_keeps_only_the_desktop_entry_section() {
        let scratch = Scratch::new("sections");
        let data_home = scratch.path().join("apps");
        std::fs::create_dir_all(data_home.join("applications")).expect("apps dir");
        std::fs::write(
            data_home.join("applications/multi.desktop"),
            "[Desktop Entry]\n\
             Name=Multi\n\
             [Desktop Action new-window]\n\
             Name=New Window\n",
        )
        .expect("write entry");

        let paths = scratch.paths_with(&[("XDG_DATA_HOME", &data_home.to_string_lossy())]);

        let fields = desktop_fields(&paths, "multi.desktop");
        assert_eq!(fields.get("Name"), Some(&"Multi".to_owned()));
        assert_eq!(fields.len(), 1);
    }

    #[test]
    fn desktop_fields_keeps_the_first_value_for_a_repeated_localised_key() {
        let scratch = Scratch::new("localised");
        let data_home = scratch.path().join("apps");
        std::fs::create_dir_all(data_home.join("applications")).expect("apps dir");
        std::fs::write(
            data_home.join("applications/localised.desktop"),
            "[Desktop Entry]\n\
             Name=English Name\n\
             Name[fr]=Nom francais\n\
             # a comment line, skipped\n\
             NoEquals\n\
             NoDisplay=false\n",
        )
        .expect("write entry");

        let paths = scratch.paths_with(&[("XDG_DATA_HOME", &data_home.to_string_lossy())]);

        let fields = desktop_fields(&paths, "localised.desktop");
        assert_eq!(fields.get("Name"), Some(&"English Name".to_owned()));
        assert_eq!(fields.get("Name[fr]"), Some(&"Nom francais".to_owned()));
        assert_eq!(fields.get("NoDisplay"), Some(&"false".to_owned()));
        assert_eq!(fields.len(), 3);
    }
}
