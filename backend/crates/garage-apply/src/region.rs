//! `apply_region()`: render the locale override and the bar's clock format.
//!
//! The renderer, [`garage_render::region::render_region`], writes `locale.env` and the
//! watched `clock-format.json` marker; nothing is signalled, because the marker's watch
//! is the bar's reload. Every region key (locale, date format, time format, first day of
//! week) reaches the bar clock this way, and the locale takes one further step through
//! [`crate::locale::apply_locale`], which is why `Route::Locale` is two apply steps and
//! `Route::Region` is one.

use garage_render::render_region;

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Render the region outputs (garage:4576-4578).
///
/// # Errors
///
/// [`ApplyError::Render`] if either output could not be written.
pub(crate) fn apply_region(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_region(cx.render())?;
    Ok(())
}
