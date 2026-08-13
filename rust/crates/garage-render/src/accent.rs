//! `render_accent()`: publish the accent for the shell to read. The file half of the accent
//! route.
//!
//! `Route::Accent`'s only step is an apply -- there is no
//! [`RenderStep`](garage_core::schema::routes::RenderStep) variant for it, because the
//! accent's applier does the writing itself before it pushes: the Python's `apply_accent()`
//! calls `render_accent()` then `push_accent()` in that order, and the Rust port keeps the
//! same shape by having the apply-side stub reach this function directly rather than through
//! [`crate::dispatch`]. This module is still reached from [`crate::all::render_all`], which
//! runs it unconditionally along with every other renderer.
//!
//! Narrow by design: publishing the accent touches no palette. The rest of the theme is
//! derived from the resolved scheme alone, so a render this small must not drag
//! [`crate::theme::render_theme`] in behind it, or picking a new accent colour would rewrite
//! a dozen unrelated toolkit configs for nothing.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the accent marker the shell and `push_accent()` both read.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_accent(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_accent"))
}
