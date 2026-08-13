//! `RenderError`: what a renderer can fail with.

use thiserror::Error;

/// Why a renderer could not complete.
///
/// [`PortPending`](RenderError::PortPending) is this crate's scaffold state. Every renderer
/// named in [`crate::dispatch`] and [`crate::all`] exists today as a stub that names the
/// Python function it stands in for and returns this variant unconditionally -- see the
/// module docs across this crate for what each one will become. Phase 3 replaces each
/// stub's body with a real implementation, and gives this enum the variants a real
/// filesystem write can actually produce: an unwritable path, a malformed fragment `luac`
/// rejected, and so on. Until then a stub has exactly one honest way to fail, so the enum
/// has exactly one variant.
#[derive(Debug, Error)]
pub enum RenderError {
    /// This renderer has not been ported yet. The `&'static str` names the Python function
    /// this stub stands in for, so a caller sees which one is still owed rather than a bare
    /// "not implemented".
    #[error("{0} has not been ported yet")]
    PortPending(&'static str),
}
