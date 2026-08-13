//! `RenderError`: what a renderer can fail with.

use garage_core::fs::atomic::AtomicWriteError;
use garage_core::fs::lua::LuaWriteError;
use garage_core::fs::marker::MarkerWriteError;
use thiserror::Error;

/// Why a renderer could not complete.
///
/// [`PortPending`](RenderError::PortPending) is this crate's scaffold state, still carried
/// by every renderer [`crate::preferences`], [`crate::idle`], [`crate::motion`] and
/// [`crate::accent`] have not replaced -- see the module docs across this crate for what
/// each one will become. The other three variants are the real filesystem failures those
/// four renderers can now produce: an unwritable path, a malformed fragment `luac` rejected,
/// a marker whose symlink could not be removed. Each wraps the writer's own error type
/// rather than flattening it, so a caller sees exactly which path and which underlying
/// `io::Error` failed.
#[derive(Debug, Error)]
pub enum RenderError {
    /// This renderer has not been ported yet. The `&'static str` names the Python function
    /// this stub stands in for, so a caller sees which one is still owed rather than a bare
    /// "not implemented".
    #[error("{0} has not been ported yet")]
    PortPending(&'static str),

    /// A generated Lua fragment could not be staged, checked or installed --
    /// [`crate::preferences::render_preferences`]'s `hyprland.lua` fragment.
    #[error(transparent)]
    Lua(#[from] LuaWriteError),

    /// A watched marker could not be written in place -- the material, accent or reduce
    /// motion markers.
    #[error(transparent)]
    Marker(#[from] MarkerWriteError),

    /// A whole-file rewrite could not be completed -- `hypridle.conf`.
    #[error(transparent)]
    Atomic(#[from] AtomicWriteError),
}
