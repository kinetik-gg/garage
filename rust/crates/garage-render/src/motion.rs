//! `render_motion()`: publish Reduce Motion for the parts of the desktop Hyprland cannot reach.
//!
//! The compositor has its own switch, and [`crate::lua::emit`]'s `motion_lua()` throws it
//! from inside [`crate::preferences::render_preferences`]. Nothing else does: waybar is a
//! separate process with a stylesheet of its own, and the shell's palettes animate
//! themselves in QML because the entrance they want is not one a Hyprland layer rule can
//! describe. Both read this marker instead of asking the compositor.
//!
//! This is the bar's own Reduce Motion, distinct from the compositor's: waybar cannot see
//! Hyprland's animations switch, so its stylesheet is rewritten and re-read whenever this
//! setting moves -- see `Route::Motion`'s four steps, which render this, render the
//! preferences fragment, then push both the compositor and the bar style.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the Reduce Motion marker waybar and the shell both watch.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_motion(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_motion"))
}
