//! `render_region()`: publish the locale override and the bar's clock, as generated fragments.
//!
//! Both belong in files this crate cannot write directly: `~/.config/uwsm/env` and waybar's
//! `config.jsonc` are stow symlinks into the dotfiles repo, and rewriting either on a
//! settings change would edit a tracked file. So each one reaches in from its own side
//! instead -- the env file sources a generated `locale.env`, `config.jsonc` includes a
//! generated `waybar-clock.jsonc` -- and only the generated halves move.
//!
//! No export at all for an empty locale override, rather than exporting the system value:
//! the point of an empty override is that the session is left to resolve `LANG` the way it
//! would with the file absent, and exporting the resolved value would pin it instead.
//!
//! The bar clock is the one part of a locale choice that can be honoured before the next
//! login: its `std::locale` is handed in explicitly rather than read from the process
//! environment, so the weekday and month names move immediately rather than waiting for a
//! relaunch.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the locale override and the bar's clock format.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_region(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_region"))
}
