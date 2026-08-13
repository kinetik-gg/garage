//! `apply_border()`: push the window border straight into the running compositor.
//!
//! A single [`crate::eval`] call over the general options [`garage_render`]'s
//! `border_general()` builds -- the size and the theme-resolved colour together, since
//! `decorations.lua` paints both borders fully transparent and a size with no colour would
//! silently shrink the window and draw nothing. A failed `eval` falls back to `hyprctl
//! reload`, the same fallback every plugin-adjacent live push in this crate shares.

use garage_render::border_colors;
use garage_render::lua::emit::border_general;
use garage_render::theme::resolve_theme;

use crate::command::run;
use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::eval::eval_config;

/// Push the window border size and colour into the running compositor (garage:4779-4782).
///
/// # Errors
///
/// Never: a refused eval falls back to `hyprctl reload` and a refused reload is swallowed,
/// exactly as the Python's two bare `run()` calls do. The `Result` is the dispatch table's
/// shape, not this applier's own.
/// The `Result` is [`crate::dispatch::run_apply`]'s uniform shape rather than this applier's
/// own: every arm of that match has to have one type, and an applier that cannot fail still
/// has to say so in the same words as one that can.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply_border(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    let prefs = cx.render().prefs();
    let (active, inactive) = border_colors(resolve_theme(prefs));
    let body = border_general(prefs.appearance.border_size.get(), &active, &inactive);
    if eval_config(cx, &format!("general = {{{body}}}")).status != 0 {
        drop(run(cx, &["hyprctl", "reload"]));
    }
    Ok(())
}
