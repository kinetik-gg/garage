//! `apply_border()`: push the window border straight into the running compositor.
//!
//! A single [`crate::eval`] call over the general options [`garage_render`]'s
//! `border_general()` builds -- the size and the theme-resolved colour together, since
//! `decorations.lua` paints both borders fully transparent and a size with no colour would
//! silently shrink the window and draw nothing. A failed `eval` falls back to `hyprctl
//! reload`, the same fallback every plugin-adjacent live push in this crate shares.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Push the window border size and colour into the running compositor.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_border(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_border"))
}
