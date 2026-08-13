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
}
