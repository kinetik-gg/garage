//! `render_all()`: write every generated fragment from the stored configuration.
//!
//! Pure, and that is the contract rather than a happy accident: this writes files and
//! signals nothing. No `gsettings`, no service restart, no `pkill`, no `hyprctl eval` or
//! `reload`. Everything that moves the running desktop lives on the apply side --
//! `push_accent()`, `push_corner_radius()`, `push_theme()`, `apply_wallpaper()`,
//! `apply_night_shift()` -- and `garage-apply`'s `apply_preferences()` is where the two
//! halves are put back together.
//!
//! # The regression this split exists to prevent
//!
//! `render_all()` used to apply as well, which made every caller an applier whether it wanted
//! to be or not: `hypridle.service`'s `ExecStartPre` ran `garage render`, so restarting the
//! locker to pick up a new lock timeout also re-themed every toolkit, pushed `gsettings` and
//! reloaded the compositor -- and at session start it did all of that a second time, once
//! from that unit and once from the `garage apply` in `autostart.lua`.
//!
//! [`RenderCx`] and `garage-apply`'s `SessionCx` are what make that mistake impossible to
//! reintroduce rather than merely undesirable: a renderer has no field, no dependency edge
//! and no widening operation that would let it reach a [`Runner`](garage_core::traits::Runner)
//! back, so `render_all()` cannot grow a call to one the way it once did. Here that mistake
//! does not compile.
//!
//! `hyprctl monitors` still runs from the workspace renderer, through
//! [`RenderCx::monitors`]'s question rather than an instruction: the plan cannot be written
//! without knowing which displays exist. Nothing here answers back.

use crate::bar::{widgets as bar_widgets, workspaces as bar_workspaces};
use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::workspaces::plan;
use crate::{
    accent, corner, displays, general, keybinds, motion, preferences, region, theme, wallpaper,
};

/// Write every generated fragment, in the Python's `render_all()` order.
///
/// `render_search_engine()` and `render_idle()` are not called from here, exactly as they are
/// not in the Python: the first is reached from [`theme::render_theme`], the second from
/// [`preferences::render_preferences`], and both are also reachable directly through
/// [`crate::dispatch::run_render`] for the routes that need only one of them.
///
/// # Errors
///
/// The first renderer's error, in call order below. [`preferences::render_preferences`],
/// [`motion::render_motion`], [`accent::render_accent`], [`theme::render_theme`] and
/// [`wallpaper::render_wallpaper`] are real; every other renderer in this chain is still a
/// stub, so a call today fails on [`keybinds::render_keybinds`], the next one in line.
///
/// [`wallpaper::render_wallpaper`]'s own answer -- whether `hyprpaper.conf` moved, and so
/// whether the service has to be restarted -- is dropped here exactly as the Python drops
/// it: `render_all()` is not the caller that restarts anything, and `garage-apply`'s
/// `apply_preferences()` is the one that asks for the flag.
pub fn render_all(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    preferences::render_preferences(cx)?;
    keybinds::render_keybinds(cx)?;
    general::render_general(cx)?;
    region::render_region(cx)?;
    plan::render_workspaces(cx)?;
    bar_workspaces::render_bar_workspaces(cx)?;
    bar_widgets::render_bar_widgets(cx)?;
    motion::render_motion(cx)?;
    accent::render_accent(cx)?;
    corner::render_corner_radius(cx)?;
    theme::render_theme(cx)?;
    let _moved = wallpaper::render_wallpaper(cx)?;
    displays::render_displays(cx)?;
    Ok(())
}
