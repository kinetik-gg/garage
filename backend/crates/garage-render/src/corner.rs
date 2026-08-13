//! `render_corner_radius()`: publish the corner radius for the shell to read. The file half.
//!
//! The rounding in px every rounded surface on the desktop is sized from: a window takes
//! Hyprland's own `decoration:rounding`, Kinetik Glass rounds the layers it decorates from
//! its own plugin options, hyprexpo rounds its overview tiles, and the shell draws its own
//! silhouettes in QML. All four are separate code paths that have to be handed the same
//! number -- see [`crate::lua::emit`]'s `corner_rounding()`, which every one of them reads
//! through -- or the corners visibly disagree with each other.
//!
//! Quickshell watches this marker, so its surfaces re-round with no restart. The world half,
//! `push_corner_radius()`, pushes the same number into the running compositor with `hyprctl
//! eval` and lives on the apply side; this half only ever writes the file.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the corner radius marker the shell reads.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_corner_radius(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_corner_radius"))
}
