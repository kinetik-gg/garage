//! Writing layer 2 back: the one writer of `preferences.toml`.

use garage_core::fs::atomic::atomic_write;
use garage_core::paths::Paths;
use garage_core::schema::Preferences;

use crate::doc::{emit_document, preference_document, preferences_table};
use crate::error::PrefsError;
use crate::load::shipped_defaults;
use crate::lock::PrefLock;

/// Write layer 2 from an effective configuration.
///
/// The one writer of `preferences.toml`, so the departures-only rule cannot be forgotten by
/// a second one. `config` is what a load produced and a caller changed -- the merged,
/// validated whole -- and what lands on disk is the difference between that and layer 1.
///
/// The merge base is read again rather than taken as an argument: the callers all loaded it
/// a moment ago, but passing it through would make it possible for a future one to subtract
/// something other than what it merged.
///
/// # The lock is the proof, not the mechanism
///
/// `_lock` is never touched. Nothing is locked *here*, exactly as in the Python -- the read
/// this is the write half of happened before the call, and every caller holds
/// [`PrefLock`] across the whole read-modify-write. Taking it again here would be worse than
/// useless: `flock` is per open file description, so a second acquire inside the same
/// process is granted immediately and would make a caller that had *not* taken it look
/// serialised. Asking for the token instead moves the requirement into the type system,
/// where the compiler refuses the call that forgot to take it.
///
/// **Parity gap, stated plainly:** the Python's `config` is a dict, so it can still be
/// carrying keys the schema does not have -- a load whose compaction was skipped merges them
/// straight through -- and this is where it reports and drops them
/// (`f"{name} is not a preference this build has; dropping it"`). A [`Preferences`] cannot
/// hold one, so that note is unreachable from a Rust save. The file that lands on disk is
/// identical either way: the departures walk is driven by layer 1, so an unknown key could
/// never have reached the output.
///
/// # Errors
///
/// [`PrefsError::Defaults`] or [`PrefsError::Unreadable`] if layer 1 cannot be re-read,
/// [`PrefsError::Emit`] if a value cannot be written as TOML, and [`PrefsError::Write`] if
/// the file cannot be replaced -- the Python's bare `OSError` out of `atomic_write()`.
pub fn save_preferences(
    paths: &Paths,
    config: &Preferences,
    _lock: &PrefLock,
    sink: Option<&mut Vec<String>>,
) -> Result<(), PrefsError> {
    let defaults = shipped_defaults(paths)?;
    let document = preference_document(&preferences_table(config), defaults.values(), sink);
    Ok(atomic_write(
        &paths.host.preferences,
        &emit_document(&document)?,
    )?)
}
