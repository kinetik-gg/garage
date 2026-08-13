//! `render_search_engine()`: publish the resolved search URL template.
//!
//! The launcher reads a resolved template, so it never has to know the engine list or how a
//! custom entry is stored. This is the whole of what a search change needs: it touches no
//! palette, so it must not drag [`crate::theme::render_theme`] in behind it -- and yet
//! `render_theme()` calls this renderer itself, because the launcher's marker still has to
//! exist and stay current across a theme switch even though the search engine did not move.
//!
//! A render, not an apply, and named for it: the launcher opens the marker on every press and
//! the shell watches it, so writing the file *is* the change landing. There is no world half
//! to pair this with, which is why
//! [`RenderStep::SearchEngine`](garage_core::schema::routes::RenderStep::SearchEngine) is the
//! entire body of `Route::Search`.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the resolved search URL template for the launcher to read.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_search_engine(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_search_engine"))
}
