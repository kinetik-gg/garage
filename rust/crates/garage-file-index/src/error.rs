//! [`FileIndexError`] -- every failure this binary's four commands can report.
//!
//! The Python names its counterpart `IndexError`, which is also the name of a builtin
//! `KeyError`-adjacent exception -- `list[99]` on a short list raises the real one, and the
//! Python module shadows it at module scope for the whole file. That is confusing enough to
//! be worth naming here: this type is called [`FileIndexError`] rather than carrying the
//! Python's name forward, which is the one deliberate naming deviation in this port.
//!
//! Every variant's [`std::fmt::Display`] is what ends up in the JSON envelope's `"error"`
//! field, via [`crate::json::response`]. Only [`FileIndexError::Usage`] is guaranteed to
//! read identically to the Python's message on both sides -- it is a literal string, ported
//! verbatim. The others wrap [`std::io::Error`], [`rusqlite::Error`] and an integer parse
//! failure, whose `Display` text is this binding's own rather than `CPython`'s
//! `OSError`/`sqlite3.Error`/`ValueError` text. See the module-level port report for why that
//! gap is left open rather than closed: none of the three happy paths the byte-parity check
//! exercises (`refresh`, `search`, `status` against a healthy database) ever formats one.

use std::io;

use thiserror::Error;

/// The literal usage line `main()` raises `IndexError` with in the Python, ported
/// character for character since it is read by a person typing the command wrong.
pub(crate) const USAGE: &str = "Usage: garage-file-index [run|refresh|search QUERY [LIMIT]|status]";

/// Every failure [`crate::dispatch`] can report through the JSON envelope.
#[derive(Debug, Error)]
pub(crate) enum FileIndexError {
    /// An unrecognised subcommand -- the Python's shadowed `IndexError`.
    #[error("{0}")]
    Usage(&'static str),
    /// A filesystem failure: reading a preferences file, creating a directory, taking the
    /// scan lock, or reading a directory entry outside the tolerated cases already handled
    /// inline (missing, permission-denied, not-a-directory).
    #[error("{0}")]
    Io(#[from] io::Error),
    /// A `SQLite` failure, from either connection.
    #[error("{0}")]
    Sqlite(#[from] rusqlite::Error),
    /// An integer argument or stored value did not parse -- the Python's `int(...)` raising
    /// `ValueError`, from either `search QUERY LIMIT`'s `argv[3]` or a `metadata` row
    /// [`crate::status::status`] expects to hold a decimal integer.
    #[error("{0}")]
    ParseInt(#[from] std::num::ParseIntError),
}

impl From<rustix::io::Errno> for FileIndexError {
    /// `flock`'s own failures surface as [`FileIndexError::Io`], through the same
    /// `Errno` -> [`io::Error`] conversion `rustix` already provides -- one place for it,
    /// rather than a `map_err` at every call site in [`crate::refresh`] and
    /// [`crate::status`].
    fn from(source: rustix::io::Errno) -> Self {
        Self::Io(source.into())
    }
}
