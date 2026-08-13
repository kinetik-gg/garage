//! `apply_motion()`: push the motion settings straight into the running compositor.
//!
//! Not through [`crate::eval`]'s `eval_config()`: that wraps a single `hl.config()` table,
//! and the per-leaf animation speeds are top-level `hl.animation()` calls instead. It also
//! needs none of the blur write-back `eval_config()` appends -- that nudge exists to make the
//! glass plugin repaint, and nothing here touches the plugin. A failed `hyprctl eval` falls
//! back to a full `hyprctl reload`, the same fallback shape as the plugin-adjacent pushes.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Push the animation switch and per-leaf speeds into the running compositor.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_motion(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_motion"))
}
