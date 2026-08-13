//! v4's step: carrying the user-owned files over from the old config root.
//!
//! Alone in a module of its own because it is alone in the migration too. Every other step
//! is handed a parsed `preferences.toml` and works on values; this one has to run before
//! that file is read at all, from a path the loader no longer looks at, and it reads the
//! version stamp itself to decide whether to. See [`migrate_config_root`] for why that
//! ordering is forced rather than chosen.

use std::fs;
use std::path::Path;

use garage_core::paths::Paths;

use crate::error::PrefsError;
use crate::load::load_toml;
use crate::migrate::schema_version;

/// The files that belong to the user rather than to the machine -- the whole of what v4 has
/// to carry across. Each is mapped to the path this process would read it from, so the move
/// lands where the session is actually looking.
///
/// Where layer 2 was kept before the rename is [`Paths::legacy_root`].
fn migrated_files(paths: &Paths) -> [(&'static str, &Path); 4] {
    [
        ("preferences.toml", &paths.host.preferences),
        ("displays.toml", &paths.host.displays),
        ("keybindings.toml", &paths.host.keybindings),
        ("workspace-blocks.toml", &paths.host.workspace_blocks),
    ]
}

/// v4: carry the user-owned files over from `~/.config/workstation`.
///
/// A step of the same migration the version stamp drives, and gated on the same stamp, but
/// it cannot live in [`migrate_preferences`]: the files have to be at their new home before
/// `preferences.toml` is read at all, and by the time that function is handed a table the
/// read has already happened -- from the new path, which would still be empty. So the stamp
/// is read from the old file here.
///
/// Only real files move. `preferences.defaults.toml` in the old directory is a stow symlink
/// into the dotfiles checkout -- layer 1, not the user's -- and following it would drag a
/// tracked file out of the repo. `generated/` is left behind for the mirror-image reason: it
/// is layer 3, machine-written, and `garage render` rewrites it under
/// [`Paths::state_root`](garage_core::paths::Paths::state_root) anyway.
///
/// Idempotent, and a no-op on the fresh install where the old directory never existed at
/// all: a second run finds nothing left to move.
///
/// # Called once per process, ahead of the dispatch
///
/// Not from inside a loader. `action keybind.rebind` and `display-test` reach
/// `keybindings.toml` and `displays.toml` without ever loading the preferences, and either
/// one arriving first would write a fresh file at the new path while the user's own was
/// still sitting at the old one.
///
/// # Errors
///
/// [`PrefsError::Move`] if a file cannot be moved or its new directory cannot be created.
/// The Python lets that `OSError` escape as well, and `main()` catches it beside
/// `SettingsError`: a half-finished move is worth reporting, because the file is the user's.
/// An unparseable old `preferences.toml` is *not* an error -- see below.
pub fn migrate_config_root(paths: &Paths) -> Result<(), PrefsError> {
    if !paths.legacy_root.is_dir() || paths.legacy_root == paths.root {
        return Ok(());
    }
    // Unparseable is "older than current" rather than fatal. The file is still the user's,
    // and it is worth more at the new path -- where the loader can report it once -- than
    // stranded at the old one.
    let version = load_toml(&paths.legacy_root.join("preferences.toml"))
        .map_or(1, |stored| schema_version(&stored));
    if version >= 4 {
        return Ok(());
    }
    for (name, target) in migrated_files(paths) {
        carry_over(paths, name, target)?;
    }
    // Emptied by the moves, so nothing is left to keep it. Anything still in there -- the
    // defaults symlink, generated/, a file dropped by hand -- and the removal refuses, which
    // is exactly the wanted outcome: the directory stays as it is rather than being cleared
    // out.
    drop(fs::remove_dir(&paths.legacy_root));
    Ok(())
}

/// One file's move, with every condition that can call it off.
fn carry_over(paths: &Paths, name: &str, target: &Path) -> Result<(), PrefsError> {
    // An env-overridden path is not the host config root: a second profile or a test harness
    // pointing elsewhere must not reach in and move the real session's files out from under
    // it.
    if target.parent() != Some(paths.root.as_path()) {
        return Ok(());
    }
    let source = paths.legacy_root.join(name);
    if is_symlink(&source) || !source.is_file() {
        return Ok(());
    }
    // Never over a file already at the new location. That one is what the session has been
    // reading, so the old one is the stale copy. `symlink_metadata` answers for the Python's
    // `target.exists() or target.is_symlink()` in one call: between them those two cover
    // every path that is *there*, following the link or not.
    if target.symlink_metadata().is_ok() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| PrefsError::Move {
            from: paths.legacy_root.join(name),
            to: target.to_path_buf(),
            source,
        })?;
    }
    fs::rename(&source, target).map_err(|error| PrefsError::Move {
        from: source,
        to: target.to_path_buf(),
        source: error,
    })
}

/// Whether a path is a symlink, without following it. A path that cannot be stat'd at all is
/// not one, which is also what Python's `Path.is_symlink()` answers.
fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
}
