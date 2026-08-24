//! `garage repair`: the way back from a `preferences.toml` this build cannot parse.
//!
//! The one user file deliberately left read-only in that state. Everything else about layer
//! 2 heals itself: a value out of range is coerced with a note, an unknown key is dropped, a
//! withdrawn spelling is carried across. But a file that is not TOML at all has no values to
//! coerce, and every writer loads the file before it writes -- which means the settings pane
//! cannot correct the very file that is blocking it. That is deliberate (guessing at what a
//! half-typed file meant would be worse than refusing it) and it is what leaves the gap this
//! command fills.
//!
//! Only `preferences.toml`, never the other three user files: those are records rather than
//! settings, and each already has its own way back on its own -- an unconfirmed display
//! layout reverts through [`crate::displays::transaction`]'s watchdog, `binds.lua`'s rescue
//! shortcuts never consult `keybindings.toml` at all, and a workspace block that cannot be
//! read is simply handed out again by
//! [`garage_render::workspaces::blocks`]. None of them can lock the user out, so none of them
//! needs a command -- and a `repair` that quietly reset all four would take the user's
//! shortcuts away to fix their wallpaper.
//!
//! [`repair_preview`] (no arguments) explains exactly what `--reset` would do and changes
//! nothing: a command that resets the user's settings does not get to do it because they
//! typed its name once. The first run is the explanation, the second run is the consent.
//! [`repair_reset`] backs the file up under a name that is never reused -- `O_EXCL` in a loop
//! against a whole-second timestamp, so two repairs in the same second cannot clobber each
//! other's backup -- writes a fresh stamp-only file, and proves it loads, all under
//! [`PrefLock`] since the swap is a read-modify-write like any other and `set` may be
//! running in the pane at the same moment.
//!
//! Takes `argv` and returns an exit code, prints lines rather than the JSON response
//! envelope, and is dispatched ahead of the JSON command path the same way [`crate::doctor`]
//! and [`crate::update`] are.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use garage_core::fs::atomic::atomic_write;
use garage_core::paths::Paths;
use garage_prefs::doc::emit_document;
use garage_prefs::{load_preferences, PrefLock, PREFERENCES_VERSION};

use crate::doctor::{local_backup_stamp, now_seconds, tilde};
use crate::error::ApplyError;

/// What `preferences.toml` is right now, without changing it in any way.
///
/// A plain TOML parse rather than the load path: the load path migrates, and v5's migration
/// *rewrites the file*. A command whose whole first mode is "act on nothing and tell me what
/// you see" cannot use a reader that writes.
#[derive(Debug, Clone, Default)]
struct PreferencesState {
    /// Whether the file is there at all.
    exists: bool,
    /// Whether it parses as TOML. False whenever it could not be read either.
    parses: bool,
    /// The parser's or the operating system's complaint, empty when there is none.
    error: String,
    /// Size in bytes, zero when the file could not be read.
    size: usize,
    /// `mtime` as local ISO 8601, empty when the file could not be read.
    modified: String,
}

fn preferences_state(path: &Path) -> PreferencesState {
    if !path.exists() {
        return PreferencesState::default();
    }
    let (Ok(metadata), Ok(raw)) = (fs::metadata(path), fs::read(path)) else {
        let error = fs::read(path)
            .err()
            .map_or_else(String::new, |error| error.to_string());
        return PreferencesState {
            exists: true,
            error,
            ..PreferencesState::default()
        };
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or_else(String::new, |since| {
            crate::doctor::local_iso8601(i64::try_from(since.as_secs()).unwrap_or(0))
        });
    let mut state = PreferencesState {
        exists: true,
        parses: true,
        error: String::new(),
        size: raw.len(),
        modified,
    };
    match std::str::from_utf8(&raw) {
        Err(error) => {
            state.parses = false;
            state.error = error.to_string();
        }
        Ok(text) => {
            if let Err(error) = text.parse::<toml::Table>() {
                state.parses = false;
                state.error = error.to_string();
            }
        }
    }
    state
}

/// Copy the current `preferences.toml` aside, under a name never used before.
///
/// `O_EXCL` in a loop rather than "does it exist?" then write, and the difference is the whole
/// value of the function: the name carries a whole-second timestamp, so two repairs in the
/// same second collide, and the check-then-write version of this would overwrite the first
/// backup with the second -- destroying the file the user ran the command to preserve. The
/// suffix counts up until the open succeeds, so an existing backup can never be clobbered, by
/// this run or by any other process racing it.
///
/// `stamp` is an argument rather than a clock read for the same reason the Python's test
/// monkeypatches `BACKUP_STAMP`: the colliding case is the one worth asserting and the one a
/// test cannot schedule.
fn backup_preferences(
    path: &Path,
    data: &[u8],
    mode: u32,
    stamp: &str,
) -> Result<PathBuf, ApplyError> {
    let stem = format!("{}.bak-{stamp}", file_name(path));
    let mut candidate = path.with_file_name(&stem);
    let mut counter = 2;
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&candidate)
        {
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                candidate = path.with_file_name(format!("{stem}-{counter}"));
                counter += 1;
            }
            Err(error) => return Err(ApplyError::Settings(error.to_string())),
            Ok(mut sink) => {
                sink.write_all(data)
                    .and_then(|()| sink.flush())
                    .and_then(|()| sink.sync_all())
                    .map_err(|error| ApplyError::Settings(error.to_string()))?;
                return Ok(candidate);
            }
        }
    }
}

/// A fresh `preferences.toml`: the schema stamp, and nothing else.
///
/// Stamp-only *is* factory state under the deltas model -- layer 2 records departures from the
/// shipped defaults, so a file with no departures in it means "follow everything Garage
/// ships". Writing the defaults out here would be the v4 bug on purpose: a frozen copy of
/// today's shipped values, outranking layer 1 forever. The stamp has to be there, because a
/// file with no `[schema]` section reads as version 1 and replays every migration on the next
/// load.
fn factory_preferences() -> Result<String, ApplyError> {
    let mut schema = toml::Table::new();
    schema.insert(
        "preferences_version".to_owned(),
        toml::Value::Integer(PREFERENCES_VERSION),
    );
    let mut document = toml::Table::new();
    document.insert("schema".to_owned(), toml::Value::Table(schema));
    emit_document(&document).map_err(|error| ApplyError::Settings(error.to_string()))
}

/// `garage repair [--reset]`: put a broken `preferences.toml` back. Reports only, unless given
/// `--reset`.
///
/// # Errors
///
/// [`ApplyError::Settings`] for an argument this command does not take, and for a file that
/// exists but cannot be read to back up. Both reach the user as `garage repair: {error}` on
/// stderr.
pub fn repair(paths: &Paths, argv: &[String]) -> Result<i32, ApplyError> {
    let mut out = String::new();
    let outcome = repair_at(&mut out, paths, argv, &local_backup_stamp(now_seconds()));
    // Printed on both paths, and that is the point: the Python prints its header lines before
    // anything can fail, so a `repair` that raises has already said what it saw.
    print!("{out}");
    outcome
}

/// [`repair`] with the transcript collected rather than printed and the backup timestamp
/// handed in, which is what the fixtures drive. The stamp is an argument for the same reason
/// the Python's own test monkeypatches `BACKUP_STAMP`.
pub(crate) fn repair_at(
    out: &mut String,
    paths: &Paths,
    argv: &[String],
    stamp: &str,
) -> Result<i32, ApplyError> {
    use std::fmt::Write as _;

    let mut reset = false;
    for argument in argv {
        if argument == "--reset" {
            reset = true;
        } else {
            return Err(ApplyError::Settings(format!(
                "Usage: garage repair [--reset]  (unexpected argument: {argument})"
            )));
        }
    }
    let path = &paths.host.preferences;
    let state = preferences_state(path);
    let _ = writeln!(out, "Garage repair -- preferences.toml only\n");
    let _ = writeln!(out, "  file      {}", tilde(&paths.home, path));
    if !state.exists {
        let _ = writeln!(
            out,
            "  state     does not exist (which is what a fresh install looks like)"
        );
    } else if state.parses {
        let _ = writeln!(out, "  state     parses as TOML");
    } else {
        let _ = writeln!(out, "  state     does NOT parse: {}", state.error);
    }
    if state.exists {
        let _ = writeln!(
            out,
            "  size      {} byte(s)\n  modified  {}",
            state.size, state.modified
        );
    }
    let _ = writeln!(out);
    if reset {
        return repair_reset(out, paths, &state, stamp);
    }
    Ok(repair_preview(out, paths, &state))
}

/// The no-argument mode: say what `--reset` would do, and change nothing.
///
/// A command that resets the user's settings does not get to do it because they typed its
/// name. The first run is the explanation, the second run is the consent.
fn repair_preview(out: &mut String, paths: &Paths, state: &PreferencesState) -> i32 {
    use std::fmt::Write as _;

    let path = &paths.host.preferences;
    let name = file_name(path);
    let _ = writeln!(out, "What `garage repair --reset` would do:");
    if state.exists {
        let _ = writeln!(
            out,
            "  1. copy the file to {}.bak-<timestamp>, a name\n     \
             that is never reused -- an existing backup is never overwritten",
            tilde(&paths.home, path)
        );
    } else {
        let _ = writeln!(out, "  1. nothing to back up, because there is no file yet");
    }
    let _ = writeln!(
        out,
        "  2. write a fresh {name} carrying the schema stamp\n     \
         (preferences_version = {PREFERENCES_VERSION}) and nothing else, which is factory \
         state: the\n     file holds your departures from the shipped defaults, so no \
         departures\n     means every setting follows what Garage ships"
    );
    let _ = writeln!(
        out,
        "  3. load it back and report whether the result is healthy"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Nothing else is touched. displays.toml, keybindings.toml and\n\
         workspace-blocks.toml are records rather than settings and each has its own\n\
         way back already: an unconfirmed display layout reverts itself after 15s,\n\
         the rescue shortcuts in config/binds.lua never read keybindings.toml, and a\n\
         workspace block that cannot be read is handed out again."
    );
    let _ = writeln!(
        out,
        "\nNothing has been changed. Re-run as `garage repair --reset` to act."
    );
    0
}

/// The acting mode: back the file up, write a fresh one, prove it loads.
///
/// Under [`PrefLock`] across all three, because the swap is a read-modify-write like any
/// other and `set` may be running in the pane at the same time -- reading the old file, then
/// writing its merged result back over the fresh one. Blocking rather than non-blocking: this
/// is a person at a terminal who asked for it, so waiting for a slider to finish is right
/// where the load path's "skip it" would not be.
///
/// The confirming load cannot deadlock on the lock it is inside: the only thing on that path
/// which takes it is `compact_preferences_file()`, which is reached only below
/// [`PREFERENCES_VERSION`] and takes it non-blocking anyway.
fn repair_reset(
    out: &mut String,
    paths: &Paths,
    state: &PreferencesState,
    stamp: &str,
) -> Result<i32, ApplyError> {
    use std::fmt::Write as _;

    let path = &paths.host.preferences;
    let name = file_name(path);
    let lock = PrefLock::acquire(paths)?;
    let backup = keep_the_old_file(out, paths, state, stamp)?;
    atomic_write(path, &factory_preferences()?)
        .map_err(|error| ApplyError::Settings(error.to_string()))?;
    let _ = writeln!(
        out,
        "  written   fresh {name}, schema stamp only (preferences_version = {PREFERENCES_VERSION})"
    );
    if !confirm(out, paths, &name) {
        return Ok(1);
    }
    drop(lock);
    let _ = writeln!(out, "\nEvery setting is back at the shipped default.");
    if let Some(kept) = backup {
        let _ = writeln!(
            out,
            "Your old file is kept at\n  {}\nNothing reads it now, so anything worth keeping \
             can be copied out by hand.",
            tilde(&paths.home, &kept)
        );
    }
    let _ = writeln!(
        out,
        "Run `garage apply` to move this session onto the defaults, or log out and back in."
    );
    Ok(0)
}

/// The backup half of [`repair_reset`]: `None` when there was nothing to keep.
fn keep_the_old_file(
    out: &mut String,
    paths: &Paths,
    state: &PreferencesState,
    stamp: &str,
) -> Result<Option<PathBuf>, ApplyError> {
    use std::fmt::Write as _;

    let path = &paths.host.preferences;
    if !state.exists {
        let _ = writeln!(out, "  backup    none needed; there was no file to keep");
        return Ok(None);
    }
    let (Ok(data), Ok(metadata)) = (fs::read(path), fs::metadata(path)) else {
        let error = fs::read(path)
            .err()
            .map_or_else(String::new, |error| error.to_string());
        return Err(ApplyError::Settings(format!(
            "cannot read {} to back it up: {error}",
            tilde(&paths.home, path)
        )));
    };
    let mode = metadata.permissions().mode() & 0o777;
    let kept = backup_preferences(path, &data, if mode == 0 { 0o644 } else { mode }, stamp)?;
    let _ = writeln!(
        out,
        "  backup    {}  ({} byte(s))",
        tilde(&paths.home, &kept),
        data.len()
    );
    Ok(Some(kept))
}

/// The confirming load: `false` when even the fresh file will not load.
///
/// It cannot deadlock on the lock it is inside: the only thing on that path which takes it is
/// the v5 compaction, which is reached only below [`PREFERENCES_VERSION`] and takes it
/// non-blocking anyway.
fn confirm(out: &mut String, paths: &Paths, name: &str) -> bool {
    use std::fmt::Write as _;

    let mut notes: Vec<String> = Vec::new();
    if let Err(error) = load_preferences(paths, Some(&mut notes)) {
        // Should be unreachable: the file that was just written is this build's own output. If
        // it happens, something outside layer 2 is wrong -- an unreadable
        // preferences.defaults.toml, most likely -- and saying so beats reporting a repair
        // that did not work.
        let _ = writeln!(out, "  after     STILL BROKEN: {error}");
        let _ = writeln!(
            out,
            "\nThe fresh file does not load either, so the problem is not {name}.\n\
             Run `garage doctor` next."
        );
        return false;
    }
    if notes.is_empty() {
        let _ = writeln!(out, "  after     loads with every value in range");
    } else {
        let _ = writeln!(
            out,
            "  after     loads, with {} note(s): {}",
            notes.len(),
            notes
                .iter()
                .take(3)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    true
}

/// `path.name`, the Python's own spelling of it.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use garage_core::paths::Paths;

    use super::{factory_preferences, preferences_state};

    pub(crate) fn scratch(label: &str) -> (PathBuf, Paths) {
        let home = std::env::temp_dir().join(format!(
            "garage-repair-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&home));
        std::fs::create_dir_all(home.join(".config/garage")).expect("scratch home");
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        let paths = Paths::from_env_map(&env);
        (home, paths)
    }

    #[test]
    fn an_absent_file_is_reported_as_absent_and_nothing_else() {
        let (home, paths) = scratch("absent");
        let state = preferences_state(&paths.host.preferences);
        assert!(!state.exists);
        assert!(!state.parses);
        assert_eq!(state.size, 0);
        assert!(state.modified.is_empty());
        drop(std::fs::remove_dir_all(&home));
    }

    #[test]
    fn a_broken_file_is_read_without_being_migrated() {
        let (home, paths) = scratch("broken");
        let broken = "[appearance\naccent_color = \"teal\nnot toml at all\n";
        std::fs::write(&paths.host.preferences, broken).expect("plant");
        let state = preferences_state(&paths.host.preferences);
        assert!(state.exists);
        assert!(!state.parses);
        assert!(!state.error.is_empty());
        assert_eq!(state.size, broken.len());
        // The whole point: the file is exactly as it was.
        assert_eq!(
            std::fs::read(&paths.host.preferences).expect("read back"),
            broken.as_bytes()
        );
        drop(std::fs::remove_dir_all(&home));
    }

    #[test]
    fn factory_state_is_the_stamp_and_nothing_else() {
        assert_eq!(
            factory_preferences().expect("the stamp emits"),
            "[schema]\npreferences_version = 6\n"
        );
    }
}
