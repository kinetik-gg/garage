//! `ApplyError`: what an applier can fail with.

use thiserror::Error;

/// Why an apply step could not complete.
///
/// The same scaffold state as [`garage_render::error::RenderError`], for the same reason:
/// every function named in [`crate::dispatch`] exists today as a stub that names the Python
/// function it stands in for and returns this variant unconditionally. Phase 3 gives each
/// stub a real body and this enum the variants a real `hyprctl`, `gsettings` or `systemctl`
/// call can actually produce -- see [`garage_core::traits::RunError`], which most of them
/// will wrap.
#[derive(Debug, Error)]
pub enum ApplyError {
    /// This applier has not been ported yet. The `&'static str` names the Python function
    /// this stub stands in for, so a caller sees which one is still owed rather than a bare
    /// "not implemented".
    #[error("{0} has not been ported yet")]
    PortPending(&'static str),

    /// A renderer an applier ran first failed. Most appliers are a render followed by a push,
    /// and the render half's failures are already modelled -- re-describing them here would
    /// be a second copy that could disagree with the first.
    #[error(transparent)]
    Render(#[from] garage_render::error::RenderError),

    /// A display layout the apply path refuses, in the Python's own words:
    /// `"At least one display must remain enabled"`, `"At least one display must show the
    /// desktop rather than mirror another"`, `"{a} overlaps {b}; drag their edges apart
    /// before applying"`, `"Display layout contains gaps; connect every monitor
    /// edge-to-edge"`, `"Display confirmation token expired"`, and the two
    /// `Unsupported TOML value` refusals `layout_toml()` can raise through its emitter.
    ///
    /// Every one of them is a `SettingsError` there, whose `str()` is the envelope's `error`
    /// field, so the text is the whole contract.
    #[error("{0}")]
    Layout(String),

    /// A mirror `apply_display_layout()` has to refuse rather than quietly drop -- Hyprland's
    /// `setMirror` would log and ignore it, leaving the screen disagreeing with the file.
    /// Text owned by [`garage_render::displays::MirrorRefusal`].
    #[error(transparent)]
    Mirror(#[from] garage_render::displays::MirrorRefusal),

    /// `int()` or `float()` refused a value in a display record. Text owned by
    /// [`garage_render::displays::NumberError`].
    #[error(transparent)]
    Number(#[from] garage_render::displays::NumberError),

    /// The TOML emitter refused a value on the way into `displays.toml`.
    #[error(transparent)]
    Emit(#[from] garage_core::toml_emit::EmitError),

    /// A file this crate rewrites whole -- `displays.toml`, the pending-transaction file --
    /// could not be replaced.
    #[error(transparent)]
    Atomic(#[from] garage_core::fs::atomic::AtomicWriteError),

    /// The display transaction lock could not be taken. A different lock from the one
    /// [`ApplyError::Lock`] carries: `DISPLAY_LOCK` serialises `display_finish()` against
    /// `initialize_display_config()`, and has nothing to do with `PREFERENCES_LOCK`.
    #[error(transparent)]
    DisplayLock(#[from] crate::displays::transaction::DisplayLockError),

    /// The pending-transaction file could not be read as JSON -- `json.loads()` raising
    /// `JSONDecodeError`, which `main()` catches beside `SettingsError`.
    ///
    /// **Parity gap, stated plainly:** the shape is the Python's and the wording is
    /// `serde_json`'s. Nothing matches on either; both are printed once.
    #[error("{0}")]
    Json(#[from] serde_json::Error),

    /// The watchdog could not be started, or an I/O call outside the writers above failed.
    /// The Python's bare `OSError`, which `main()` catches beside `SettingsError`.
    #[error("{0}")]
    Io(String),

    /// A command that was supposed to move the session refused to.
    ///
    /// `run_or_raise()`'s `SettingsError(result.stderr.strip() or message)`: the command's own
    /// complaint when it had one, and the step's message when it did not. Both halves reach
    /// the user's stderr verbatim, which is why the choice between them is made here rather
    /// than by whoever prints it.
    #[error("{0}")]
    Signal(String),
}
