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

    /// A command that was supposed to move the session refused to.
    ///
    /// `run_or_raise()`'s `SettingsError(result.stderr.strip() or message)`: the command's own
    /// complaint when it had one, and the step's message when it did not. Both halves reach
    /// the user's stderr verbatim, which is why the choice between them is made here rather
    /// than by whoever prints it.
    #[error("{0}")]
    Signal(String),
}
