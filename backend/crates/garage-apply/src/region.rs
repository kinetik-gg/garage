//! `apply_region()`: render the locale override and the bar's clock, then reload the bar.
//!
//! The renderer, [`garage_render::region::render_region`], writes both generated fragments;
//! this only adds the signal -- `reload_bar()`, the same `pkill -USR2 waybar` every bar-owned
//! change in this crate shares. Every region key (locale, date format, time format, first day
//! of week) reaches the bar clock this way, and the locale takes one further step through
//! [`crate::locale::apply_locale`], which is why `Route::Locale` is two apply steps and
//! `Route::Region` is one.

use garage_render::render_region;

use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::workspaces::reload_bar;

/// Render the region fragments and signal the bar to re-read them (garage:4576-4578).
///
/// # Errors
///
/// [`ApplyError::Render`] if either fragment could not be written. The signal reports
/// nothing, as `reload_bar()` never does.
pub(crate) fn apply_region(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_region(cx.render())?;
    reload_bar(cx);
    Ok(())
}
