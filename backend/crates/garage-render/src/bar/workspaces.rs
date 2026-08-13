//! `render_bar_workspaces()`: publish the bar's left side -- menu, workspaces, then media.
//!
//! A fragment of its own rather than folded into either neighbour: `waybar-clock.jsonc` is
//! written on a region change and `waybar-widgets.jsonc` on a bar change, and sharing either
//! file would make each writer responsible for reproducing the other's content or silently
//! dropping it.
//!
//! Only `modules-left` is published. `persistent-workspaces` used to be spelled out in
//! `config.jsonc`, monitor by monitor, as a second copy of the workspace plan; waybar 0.15
//! already loads persistent workspaces from Hyprland's own workspace rules -- which
//! [`crate::workspaces::plan::render_workspaces`] generates -- so the copy is gone rather
//! than generated, and the bar follows a count change with nothing left to keep in step.
//!
//! Written unconditionally, even with no plan at all: `config.jsonc` no longer names
//! `modules-left` itself, so an absent fragment would be an empty left side rather than a
//! wrong one -- there is no static fallback to fall back to.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the bar's left side: the menu, the workspace indicator, and media if enabled.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_bar_workspaces(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_bar_workspaces"))
}
