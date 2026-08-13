//! `push_corner_radius()` and `apply_corner_radius()`: the corner radius, into the running
//! compositor and back out through the render half.
//!
//! `push_corner_radius()` is not fatal when it fails: this also runs from
//! [`crate::route`]'s `apply_preferences()` at session start, where the compositor may not be
//! up yet, and the generated fragment already carries the same values into the next reload
//! regardless. Pushed through [`crate::eval`]'s `eval_config()` across the core decoration,
//! Kinetik Glass and hyprexpo plugin options together, so a plugin that never loaded takes
//! the whole `eval` down with it and the guarded fragment is the only thing left that can
//! apply the change correctly -- which is why a failed push falls back to `hyprctl reload`
//! rather than reporting an error.
//!
//! `apply_corner_radius()` is the two-step route's applier: render the marker
//! ([`garage_render::corner::render_corner_radius`], reached through
//! [`crate::cx::SessionCx::render`]), then push it live.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Render the corner radius marker, then push it into the running compositor.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_corner_radius(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_corner_radius"))
}
