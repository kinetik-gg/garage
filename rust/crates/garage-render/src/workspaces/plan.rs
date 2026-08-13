//! `render_workspaces()`: publish the workspace plan for `config.variables` to read.
//!
//! Not loaded from `hyprland.lua`'s override block the way the other generated fragments
//! are: `config.binds` is required before that block runs, and it has to build its per-slot
//! binds from the same plan the workspace rules use. `config.variables`, which is required
//! before `config.binds`, picks the fragment up instead -- the load order is the reason this
//! renderer's file sits where it does rather than beside its siblings.
//!
//! `workspace_plan()` behind it is one of two shapes. Shared mode is one unpinned group whose
//! `monitor` is left empty in the emitted table, which is what tells the Lua side to address
//! workspaces by id instead of by output. Per-display mode calls `per_display_groups()`,
//! which is documented in [`crate::workspaces::blocks`] alongside the allocator it depends
//! on -- the plan itself does not decide which ids a display owns, it only asks.
//!
//! An empty plan -- nothing detected and nothing saved -- removes the fragment rather than
//! writing one that pins workspaces to connectors that may no longer exist. Falling back to
//! no fragment at all falls back to the plan `variables.lua` already carries by default,
//! which is a plan that always has at least one group.
//!
//! `hyprctl monitors` is read from here, through [`RenderCx::monitors`]'s question rather
//! than an instruction -- see the crate's own top-level doc and [`crate::workspaces::blocks`]
//! for why the saved layout leads and the live list is only folded in on top.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the workspace plan for `config.variables` to read.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_workspaces(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_workspaces"))
}
