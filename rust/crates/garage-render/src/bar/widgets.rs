//! `render_bar_widgets()`: publish the bar's height, empty centre and its whole right side.
//!
//! A third fragment rather than more of either existing one, by the same rule
//! [`crate::bar::workspaces`] states: `waybar-clock.jsonc` is written on a region change and
//! `waybar-workspaces.jsonc` on a workspaces change, so a bar change that shared either file
//! would have to reproduce that writer's content or silently drop it. One writer per
//! fragment, three fragments.
//!
//! `height` lives here rather than in `config.jsonc`, for the reason `modules-left` is not
//! there either: an option a named file declares is won by that file, and an include can
//! never override it. Everything the bar section decides about the layout therefore has to
//! leave `config.jsonc` entirely.
//!
//! Written unconditionally, even with every widget switched off: the fragment is then the
//! bar's static tail and nothing else, which is a correct bar. An absent fragment is not --
//! `config.jsonc` no longer names `modules-right`, so it would be a bar with an empty right
//! side. `waybar.service` runs `garage render-bar` as its `ExecStartPre` for exactly that
//! reason.
//!
//! The module definitions this writes -- one `image#metric-*` module per enabled metric, the
//! AI usage strip, the media control -- are the one place a metric strip's declared `size`
//! has to agree with `garage-metrics`' own layout table; `tests/test_bar.py` parses this
//! script and fails on drift between the two.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the bar's height, empty centre and its whole right side.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_bar_widgets(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_bar_widgets"))
}
