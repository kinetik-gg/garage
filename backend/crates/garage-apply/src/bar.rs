//! `apply_bar_workspaces()`, `apply_bar_style()` and `apply_bar_widgets()`: the bar's three
//! fragments, each paired with the one signal that reloads all of them.
//!
//! All three end at `reload_bar()` -- `pkill -USR2 -x waybar`, waybar's default SIGUSR2
//! action, which re-reads the config and every file it includes -- and none of them touch the
//! compositor: the bar is a layer surface with its own config and its own stylesheet, and
//! nothing in Hyprland reads either. They stay three separate appliers because they write
//! three separate fragments: the workspaces indicator does not depend on style, style does
//! not depend on widgets, and media's definition is widget-owned while its place immediately
//! after the workspace indicator is left-side-owned -- so a media toggle alone has to
//! republish both fragments before the one reload.
//!
//! `apply_bar_style()`'s own render half, `render_bar_style()`, is narrow on purpose, the
//! same way `garage render-bar` is: `bar.padding_scale` is a slider and the pane fires a
//! `set` per notch, so going through [`garage_render::theme::render_theme`] instead would
//! rewrite twenty toolkit configs per notch, for a change none of them can see. Only this one
//! file carries `[bar]`'s own styling, built from [`garage_render`]'s `waybar_style_css()`.
//! It writes with `write_marker()` rather than an atomic rename, for the reason every path
//! under `~/.config` waybar watches does: a rename past its `inotify` watch is a change it
//! never hears about.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Publish the bar's module list (menu, workspaces, media) and signal the bar to re-read it.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_bar_workspaces(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_bar_workspaces"))
}

/// Rewrite the bar's stylesheet alone and have the bar re-read it.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_bar_style(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_bar_style"))
}

/// Republish the bar's widgets and have the bar re-read them.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_bar_widgets(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_bar_widgets"))
}
