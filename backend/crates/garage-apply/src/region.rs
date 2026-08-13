//! `apply_region()`: render the locale override and the bar's clock, then reload the bar.
//!
//! The renderer, [`garage_render::region::render_region`], writes both generated fragments;
//! this only adds the signal -- `reload_bar()`, the same `pkill -USR2 waybar` every bar-owned
//! change in this crate shares. Every region key (locale, date format, time format, first day
//! of week) reaches the bar clock this way, and the locale takes one further step through
//! [`crate::locale::apply_locale`], which is why `Route::Locale` is two apply steps and
//! `Route::Region` is one.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Render the region fragments and signal the bar to re-read them.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_region(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_region"))
}
