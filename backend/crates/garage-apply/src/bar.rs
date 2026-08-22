//! `apply_bar_layout()`: the bar's one apply step.
//!
//! It republishes the watched `bar-layout.json` marker and stops there: the shell's
//! `FileView` watch is the reload, so there is no signal, no restart and no compositor
//! contact on this route -- the bar is a layer surface of the shell's own process.
//!
//! Narrow on purpose for the old reason, kept: `bar.padding_scale` is a slider and the
//! pane fires a `set` per notch, so going through [`garage_render::theme::render_theme`]
//! instead would rewrite twenty toolkit configs per notch, for a change none of them can
//! see. This writes one small file.

use garage_render::render_bar_layout;

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Republish the bar's layout marker; the watch does the rest.
///
/// # Errors
///
/// [`ApplyError::Render`] if the marker could not be written.
pub(crate) fn apply_bar_layout(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_bar_layout(cx.render())?;
    Ok(())
}
