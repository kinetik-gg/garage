//! `render_general()`: publish what the keybindings read, as markers rather than preferences.
//!
//! Both are read on a keypress: the launcher wrapper runs on every `SUPER+Space`, and
//! Hyprland parses `binds.lua` with no way to shell out to a helper for an answer. A marker
//! costs an open and a read; resolving the launcher choice or the terminal's `Exec=` line at
//! bind time would be felt on every press instead of once per render.
//!
//! Three markers, not one: which launcher (`builtin` or `external`), the resolved terminal
//! command, and the resolved browser command. The terminal and browser are desktop-file
//! lookups -- see the (not yet ported) desktop-file resolution this will eventually call --
//! so publishing them here is what keeps a keybind's `Exec=` from having to repeat that
//! lookup on every press.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the launcher, terminal and browser markers `binds.lua` and the launcher wrapper read.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_general(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_general"))
}
