//! `resolve_theme()`, `applied_scheme()` and `render_theme()`: which palette is in effect,
//! which one is live, and writing the one that is in effect.
//!
//! `resolve_theme()` is the schedule: `light`, `dark`, or `auto`, which reads as a light
//! window between two times of day, wrapping midnight the same way the night shift schedule
//! does. `applied_scheme()` is a different question entirely, and the distinction is
//! load-bearing -- it is written by `push_theme()` and nothing else, in particular not by
//! `render_theme()`, so it is by definition the scheme currently live on the desktop rather
//! than the scheme last generated. That is what makes it usable as the change gate:
//! `ApplyStep::ThemeIfSchemeMoved` compares `resolve_theme()` against `applied_scheme()`
//! rather than against anything a render wrote, because a render that wrote the files
//! without pushing them must still leave `applied_scheme()` saying what is on screen.
//!
//! `render_theme()` itself writes every file the resolved palette decides, and is pure --
//! nothing is signalled. Only generated paths are written; everything under `~/.config` this
//! checkout tracks is a stow symlink into the dotfiles repo, so writing there would edit
//! tracked files and leave the working tree dirty on every switch. `SCHEME_FILE` is
//! deliberately *not* written from here, for the same reason `applied_scheme()` reads it
//! rather than a render output: writing it from a pure render would make the change gate
//! think the apply it is guarding had already happened.
//!
//! `render_theme()` calls [`crate::search::render_search_engine`] and the toolkit writers in
//! [`crate::palette`] as part of its own body -- the launcher marker and every toolkit config
//! have to stay current across a theme switch even when neither the search engine nor any
//! individual toolkit setting moved.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write every file the resolved palette decides: the toolkit configs, the search engine
/// marker, and the bar's foreground marker.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn render_theme(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_theme"))
}
