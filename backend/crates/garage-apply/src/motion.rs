//! `apply_motion()`: push the motion settings straight into the running compositor.
//!
//! Not through [`crate::eval`]'s `eval_config()`: that wraps a single `hl.config()` table,
//! and the per-leaf animation speeds are top-level `hl.animation()` calls instead. It also
//! needs none of the blur write-back `eval_config()` appends -- that nudge exists to make the
//! glass plugin repaint, and nothing here touches the plugin. A failed `hyprctl eval` falls
//! back to a full `hyprctl reload`, the same fallback shape as the plugin-adjacent pushes.

use garage_render::lua::emit::motion_lua;

use crate::command::run;
use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Push the animation switch and per-leaf speeds into the running compositor
/// (garage:4785-4794).
///
/// # Errors
///
/// Never, for the same reason [`crate::border::apply_border`] never does: both `run()` calls
/// are unchecked and the fallback's own failure is swallowed.
/// The `Result` is [`crate::dispatch::run_apply`]'s uniform shape rather than this applier's
/// own: every arm of that match has to have one type, and an applier that cannot fail still
/// has to say so in the same words as one that can.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn apply_motion(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    let code = motion_lua(cx.render().prefs());
    if run(cx, &["hyprctl", "eval", &code]).status != 0 {
        drop(run(cx, &["hyprctl", "reload"]));
    }
    Ok(())
}
