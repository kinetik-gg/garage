//! `CliError`: what the dispatch can fail with, and therefore what the envelope can say.
//!
//! The Python has one exception type on this boundary and its `str()` is the contract:
//! `main()` catches `(SettingsError, OSError, ValueError, json.JSONDecodeError)` and puts
//! `str(error)` straight into the envelope's `error` field. Nothing downstream matches on a
//! *type* -- the QML client reads `ok` and shows `error` -- so the only thing worth being
//! exact about is the text.
//!
//! Every variant below is therefore either a pass-through of an error whose text was already
//! pinned against the Python by the layer that owns it (`#[error(transparent)]`), or a
//! message this file spells out because the string lives in `main()` itself and nowhere
//! else. `apply_changed_preference()`'s two "Unsupported ..." refusals used to be the second
//! kind here; they moved to [`ApplyError`] with the route walk itself, so `garage set` and
//! `garage action appearance.night_shift.toggle` cannot disagree about what an unroutable key
//! is called.
//!
//! There is no scaffold variant left. Every command in the table reaches a real layer, so a
//! failure here is always something the machine or the argument said.

use garage_apply::ApplyError;
use garage_core::schema::{ParseKeyError, SetError};
use garage_prefs::{LockError, PrefsError};
use garage_render::RenderError;
use thiserror::Error;

/// Why a settings-backend command did not finish. Its `Display` is the envelope's `error`.
#[derive(Debug, Error)]
pub(crate) enum CliError {
    /// `SettingsError(f"Unknown command: {command}")`, `main()`'s final `else`.
    #[error("Unknown command: {0}")]
    UnknownCommand(String),

    /// `SettingsError("Usage: garage set KEY JSON_VALUE")`: `main()`'s `len(argv) != 4`
    /// guard, which is about the *argument count* and not about the key or the value, so it
    /// is raised before either is looked at.
    #[error("Usage: garage set KEY JSON_VALUE")]
    SetUsage,

    /// A `display-*` command invoked with no argument at all.
    ///
    /// **Deviation, stated plainly:** the Python indexes `argv[2]` without checking, so this
    /// case is an `IndexError` there -- outside `main()`'s `except` tuple, which means a
    /// traceback on stderr rather than an envelope. The exit status is 1 either way, and the
    /// shape here is the one `set`'s own argument-count guard already establishes.
    #[error("Usage: {0}")]
    DisplayUsage(&'static str),

    /// Reading, migrating or writing layer 2 failed. Text pinned by `garage-prefs`.
    #[error(transparent)]
    Prefs(#[from] PrefsError),

    /// `PREFERENCES_LOCK` could not be taken. The Python's bare `OSError` out of the
    /// `open`/`flock` pair in `main()`'s `set` branch, which `main()` catches beside
    /// `SettingsError`.
    #[error(transparent)]
    Lock(#[from] LockError),

    /// A renderer refused. Text owned by `garage-render`.
    #[error(transparent)]
    Render(#[from] RenderError),

    /// An applier refused. Text owned by `garage-apply`.
    #[error(transparent)]
    Apply(#[from] ApplyError),

    /// `set_nested()`'s `f"Unknown preference: {dotted}"`, raised by the schema's own key
    /// parse rather than restated here.
    #[error(transparent)]
    Key(#[from] ParseKeyError),

    /// `set` refused the key outright -- a key the schema declares unsettable.
    #[error(transparent)]
    Set(#[from] SetError),

    /// `json.loads(argv[3])` raised.
    ///
    /// **Parity gap, stated plainly:** the *shape* is the Python's -- a malformed value
    /// reaches the envelope as an `error` with `ok` false and exit 1 -- but the wording is
    /// `serde_json`'s, not `json`'s. Python writes `Expecting value: line 1 column 1 (char
    /// 0)`; this writes `expected value at line 1 column 1`. Both are printed once, for a
    /// person holding a bad argument, and nothing matches on either.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}
