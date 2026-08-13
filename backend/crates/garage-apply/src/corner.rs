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

use garage_render::lua::emit::{corner_rounding, python_g_format, CORNER_POWER};
use garage_render::render_corner_radius;

use crate::command::run;
use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::eval::eval_config;

/// Push the corner radius into the running compositor. The world half (garage:4756-4771).
///
/// Not fatal when it fails: this also runs from `apply_preferences()` at session start, where
/// the compositor may not be up yet, and the generated fragment carries the same values into
/// the next reload regardless.
pub(crate) fn push_corner_radius(cx: &mut SessionCx<'_>) {
    let rounding = corner_rounding(cx.render().prefs());
    let power = python_g_format(CORNER_POWER);
    let body = format!(
        "decoration = {{rounding = {rounding}, rounding_power = {power}}}, \
         plugin = {{kinetik_glass = {{layer_rounding = {rounding}, corner_power = {power}}}, \
         hyprexpo = {{tile_rounding = {rounding}, tile_rounding_power = {power}}}}}"
    );
    if eval_config(cx, &body).status != 0 {
        // A plugin that never loaded takes the whole eval down with it, so the guarded
        // fragment is the only thing that can apply this correctly.
        drop(run(cx, &["hyprctl", "reload"]));
    }
}

/// Render the corner radius marker, then push it into the running compositor
/// (garage:4774-4776).
///
/// # Errors
///
/// [`ApplyError::Render`] if the marker could not be written. The push itself reports
/// nothing -- see [`push_corner_radius`].
pub(crate) fn apply_corner_radius(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_corner_radius(cx.render())?;
    push_corner_radius(cx);
    Ok(())
}
