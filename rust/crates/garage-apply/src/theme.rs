//! `push_theme()`, `apply_theme()` and `apply_theme_if_scheme_moved()`: move the running
//! desktop onto the palette the renderer already wrote.
//!
//! `push_theme()` is the world half, and the one that publishes the scheme marker: from the
//! moment it runs, `applied_scheme()` answers with the scheme just pushed, which is true
//! because everything below it has been told about it too. It writes that marker before
//! anything else specifically so a caller reading it mid-push already sees the destination
//! rather than the source.
//!
//! The desktop picture belongs to the appearance, so it moves with the palette -- hooked here
//! because this is the one site every switch goes through: a manual change, session start,
//! and the five-minute theme timer. Gated on what `current` already resolves to rather than
//! on the scheme marker, because that marker was just overwritten a line above and can no
//! longer say what was on screen a moment ago; ungated, the timer would re-issue the
//! wallpaper -- and restart hyprpaper whenever the fit moved with it -- every five minutes
//! even when nothing changed.
//!
//! From there: GTK4 and libadwaita follow the portal setting live (`gsettings`), GTK3 and
//! `XWayland` apps re-theme when `xsettingsd` rereads its config (`reload-or-restart`), and
//! waybar, kitty and swayosd are each signalled to reread their own generated files.
//!
//! `apply_theme()` is render-then-push in one call: `render_theme()` followed by
//! `push_theme()`. `apply_theme_if_scheme_moved()` is the actual `Route::Theme` step, and the
//! one that matters for cost: rewriting a dozen toolkit configs and reloading Hyprland is
//! only worth doing when the palette actually moves, because nothing downstream reads the
//! mode or the schedule -- the renderer picks its decoration colours from the resolved scheme
//! too, so an unchanged scheme means every output would come out byte-identical. That also
//! covers switching to `auto` at night when dark is already live, since the theme timer
//! re-checks the schedule on its own five-minute interval regardless.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Re-theme the session, but only when [`garage_render::theme`]'s resolved scheme has
/// actually moved since the last push.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_theme_if_scheme_moved(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_theme_if_scheme_moved"))
}
