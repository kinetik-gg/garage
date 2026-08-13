//! `render_preferences()`: the shared fragment most appearance and input keys land in.
//!
//! One `hyprland.lua` fragment carrying three separate `hl.config()` calls: the whole
//! `[input]` section, then the theme-resolved decoration -- corner rounding, the group and
//! border colours, the material's core options -- then, guarded on `GLASS_AVAILABLE` and
//! `HYPREXPO_AVAILABLE`, the two plugins' own rounding. Guarded because setting a plugin
//! keyword for a plugin that failed to load is a config error, not a no-op.
//!
//! The glass values matter here as much as the colours: the corner rings are a white inner
//! highlight over a dark outer border, which inverts on a light desktop, and a near-black
//! tint would darken it rather than lighten it. `render_preferences()` is also the one
//! renderer that writes the material marker ([`crate::theme`] does not) so the shell has
//! something to read before it first draws, the same way the corner radius marker is
//! published from more than one place.
//!
//! Ends by rendering the idle fragment too -- in the Python, `render_preferences()` is the
//! last stop before `render_idle()` on every route that touches `[input]`, because Reduce
//! Motion and the lock timeouts live in neighbouring parts of the same generated
//! `hyprland.lua` include chain. That call has to land on the same path `garage
//! render-idle`'s re-entrant restart takes, which is why [`crate::idle`] carries its own
//! invariant about never taking the preferences lock rather than inheriting one from here.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the shared `hyprland.lua` fragment: `[input]`, the theme-resolved decoration, and
/// the two plugin blocks.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_preferences(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_preferences"))
}
