//! `PrefsError` — what reading or writing layer 2 can fail with.
//!
//! The Python raises one exception type, `SettingsError`, whose `str()` reaches the user as
//! the `error` field of the JSON envelope or as a line on stderr. So the *text* is the
//! contract, not the type, and each variant below is named after the Python expression that
//! produced it. Two of them are message-identical to the Python by construction --
//! [`PrefsError::Emit`] wraps [`EmitError`], whose text was pinned against `toml_value()` in
//! task 2.5 -- and the rest carry the same shape with a Rust-side detail where the two
//! runtimes cannot agree on the wording (see [`PrefsError::Unreadable`]).
//!
//! `OSError` has no counterpart variant of its own. The Python lets it escape from
//! `migrate_config_root()` and `save_preferences()`, and `main()` catches it beside
//! `SettingsError`; here it arrives as [`PrefsError::Move`] or [`PrefsError::Write`], which
//! are the only two places layer 2 is written from.

use std::io;
use std::path::PathBuf;

use garage_core::fs::atomic::AtomicWriteError;
use garage_core::schema::defaults::DefaultsError;
use garage_core::toml_emit::EmitError;
use thiserror::Error;

/// Why a preferences file could not be read, migrated or written.
#[derive(Debug, Error)]
pub enum PrefsError {
    /// `load_toml()`'s `SettingsError(f"{path.name}: {error}")`: the file exists but could
    /// not be opened or does not parse.
    ///
    /// **Parity gap, stated plainly:** the shape is the Python's -- the file's own name, a
    /// colon, the parser's complaint -- but the complaint itself is `toml`'s rather than
    /// `tomllib`'s, and the two spell a syntax error differently ("expected `=`, found
    /// `\n`" against "Expected '=' after a key in a key/value pair (at line 1, column 6)").
    /// Nothing downstream matches on it: it is printed, once, for a person holding a broken
    /// file.
    #[error("{file}: {detail}")]
    Unreadable {
        /// The file's name without its directory -- Python's `path.name`.
        file: String,
        /// What the parser or the operating system said.
        detail: String,
    },

    /// Layer 1 could not answer. Either the compiled copy does not parse -- a broken build --
    /// or the shipped file is missing a key the schema declares.
    ///
    /// **Parity gap:** the Python has no equivalent of the second case. A defaults file
    /// missing a key leaves that key absent from the merge, and `validate_preferences()`
    /// then coerces it onto `FALLBACK_DEFAULTS` with a note. Here layer 1 is a typed
    /// [`Defaults`](garage_core::schema::Defaults), which cannot be built with a key
    /// missing, so the load fails instead. That is task 2.4's decision, taken deliberately
    /// and documented there: the shipped file is a stow symlink into the checkout that
    /// `tests/test_schema.py` pins, so a key missing from it is a broken build rather than
    /// a session to be rescued.
    #[error(transparent)]
    Defaults(#[from] DefaultsError),

    /// `dump_toml()` refused a value: a non-finite float, or -- reachable only from a hand
    /// edit -- a TOML date, time or datetime sitting where a scalar belongs. Both messages
    /// are byte-identical to the Python's `SettingsError`.
    #[error(transparent)]
    Emit(#[from] EmitError),

    /// `atomic_write()` failed. The Python's bare `OSError` out of `save_preferences()`;
    /// the one on the *load* path, inside `compact_preferences_file()`, is swallowed there
    /// rather than reaching this type.
    #[error(transparent)]
    Write(#[from] AtomicWriteError),

    /// `os.replace()` or the `mkdir` before it failed while `migrate_config_root()` was
    /// carrying a file over from the old config root. The Python lets this `OSError` escape
    /// too: a half-finished move is worth reporting, since the file is the user's.
    #[error("{}: could not be moved to {}: {source}", from.display(), to.display())]
    Move {
        /// Where the file was.
        from: PathBuf,
        /// Where it was going.
        to: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
}

impl PrefsError {
    /// `SettingsError(f"{path.name}: {error}")` for a path and whatever complained about it.
    ///
    /// A path with no final component -- `/`, or `..` -- has an empty `path.name` in Python
    /// too, so the empty string is the faithful answer rather than a case to guard.
    pub(crate) fn unreadable(path: &std::path::Path, detail: impl std::fmt::Display) -> Self {
        Self::Unreadable {
            file: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            detail: detail.to_string(),
        }
    }
}
