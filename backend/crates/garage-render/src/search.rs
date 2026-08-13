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

use garage_core::fs::marker::write_marker;
use garage_core::schema::enums::SearchEngine;

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the resolved search URL template for the launcher to read (garage:4234-4249).
///
/// An empty template falls back to Google's rather than being published as nothing: `custom`
/// with the field not yet filled in is the state the pane sits in for as long as it takes to
/// type a URL, and a launcher handed an empty template would open nothing at all.
///
/// # Errors
///
/// [`RenderError::Marker`] if the marker could not be written.
pub(crate) fn render_search_engine(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let appearance = &cx.prefs().appearance;
    let template = if appearance.search_engine == SearchEngine::Custom {
        appearance.search_custom_url.as_str()
    } else {
        appearance.search_engine.url_template()
    };
    let resolved = if template.is_empty() {
        SearchEngine::Google.url_template()
    } else {
        template
    };
    write_marker(&cx.paths().markers.search_engine, &format!("{resolved}\n"))?;
    Ok(())
}
