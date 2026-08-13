//! `render_wallpaper()`: the generated `hyprpaper.conf`, and what it resolves the picture from.
//!
//! hyprpaper 0.8.4 reads `fit_mode` from its config file, and only at startup: its IPC
//! exposes `wallpaper` and `listactive` alone, there is no reload request and no signal
//! handler. A changed file is therefore exactly the set of cases that need the service
//! restarted, which is why the real `render_wallpaper()` reports whether it changed instead
//! of restarting anything itself -- the restart decision belongs to the apply side, which is
//! the caller that knows whether this is the only thing that moved.
//!
//! The path this fragment names is always the `current` symlink, never a resolved target,
//! because that is what a fresh session boots from; a live path change goes over hyprpaper's
//! IPC instead, on the apply side, and never touches this file at all.
//!
//! `wallpaper_target()` is the resolver behind it: a colour is an image too, since hyprpaper
//! has no colour mode of its own, so a chosen swatch is rendered to a solid PNG named after
//! it and cached under the wallpaper directory. An appearance that has never been given a
//! picture falls back to whatever `current` already points at, which keeps the desktop it
//! has rather than blanking it -- and only a first session, before any wallpaper has ever
//! been chosen, resolves to nothing at all.
//!
//! Not reached through [`RenderStep`](garage_core::schema::routes::RenderStep): none of the
//! three wallpaper routes carries a render step, because dressing the light appearance from
//! a dark session must not change what is behind the pane -- see
//! [`Route::WallpaperLight`](garage_core::schema::routes::Route::WallpaperLight) and its
//! siblings. This module's renderer is reached from [`crate::all::render_all`] and from the
//! apply side directly, never from [`crate::dispatch`].

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the generated `hyprpaper.conf` for the resolved theme's wallpaper.
///
/// The real implementation reports whether the file changed, which is what the apply side
/// needs to decide whether hyprpaper has to be restarted; this stub's fixed signature defers
/// that until Phase 3.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_wallpaper(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_wallpaper"))
}
