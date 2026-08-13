//! `render_displays()`: the per-monitor `hyprland.lua` fragment from `displays.toml`.
//!
//! Lenient on purpose, unlike the apply-side layout check: this also runs at session start
//! from a possibly hand-edited `displays.toml`, and refusing the whole fragment because one
//! monitor rule is bad would drop every monitor rule, not just the offending one. Mirror
//! targets are resolved the same lenient way -- a display that names an impossible mirror
//! (itself, a disabled output, another mirror) simply mirrors nothing here, where the
//! apply-side layout check would reject the whole layout instead.
//!
//! Mirror sources are written ahead of the mirrors that name them. Hyprland collects every
//! rule and applies them once the whole config has parsed, so the ordering makes no
//! difference to the compositor; it is for whoever reads the generated file by hand.
//!
//! A mirror is written with no position of its own: Hyprland drops a mirrored output out of
//! the monitor layout entirely and pins it onto its source, so a coordinate would only be a
//! stale value fighting that. VRR is clamped into Hyprland's `-1..3` rather than validated
//! and refused, for the same leniency reason above -- a bad VRR value on one display must not
//! take every monitor rule down with it.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the per-monitor `hyprland.lua` fragment from the saved display layout.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_displays(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_displays"))
}
