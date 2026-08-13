//! `render_keybinds()`: publish the shortcut set for `config/binds.lua` to read.
//!
//! A data file, and that is the point rather than a detail. The command a custom shortcut
//! runs is text the user typed, and generating a Lua `bind("...")` line from it would make
//! the field a place to write Lua: a command ending `")) ;` closes the call and everything
//! after it becomes config. Here the command is instead a JSON string that `binds.lua` reads
//! with a reader that can only ever return strings, so there is no route from the field to
//! the chunk.
//!
//! Reached from [`crate::all::render_all`], which calls it with the already-loaded
//! keybindings document. Loading that document -- parsing `keybindings.toml`, filtering
//! overrides against the published catalog -- is `garage-apply`'s concern, not this crate's,
//! which is why this renderer's real signature will take the document rather than only the
//! preferences its stub signature carries today.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the resolved shortcut set for `config/binds.lua` to read.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_keybinds(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_keybinds"))
}
