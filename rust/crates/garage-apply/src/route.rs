//! Walking a route: `apply_preferences()`, `apply_changed_preference()`, and the two step
//! kinds that have no module of their own to live in.
//!
//! `apply_changed_preference()` is the whole reason [`garage_core::schema::routes`] exists as
//! a typed table rather than a ladder of string prefixes. The Python version used to be
//! exactly that ladder -- `key.startswith("glass_")` and so on -- which is what let a new key
//! be added, saved, and then silently never applied, because the ladder's final `else` was
//! the only thing that would have said so. `Route::steps()` and this crate's
//! [`crate::dispatch::run_apply`] replace the ladder with an exhaustive match the compiler
//! enforces instead of a fallthrough a person has to remember to update.
//!
//! `apply_preferences()` is the session-start path, from `autostart.lua`, and the only
//! command that is supposed to touch every subsystem at once: render everything, then push
//! accent, corner radius and theme, reload the compositor, apply the wallpaper and the night
//! shift schedule, and -- unless told not to, which only the idle-route's own re-entrant
//! render asks for -- restart `hypridle.service`. `render` is the pure half of this same
//! sequence.
//!
//! # The two steps with no dedicated module
//!
//! [`ApplyStep::RunOrRaise`](garage_core::schema::routes::ApplyStep::RunOrRaise) is not a
//! named Python function call in the same sense as the rest of the table -- it is the one
//! step whose whole behaviour (which command, which failure message) is carried as data on
//! the step itself. `run_or_raise()` is what turns that data into a call: run the command,
//! and raise with the message if it fails. It lives here because it is route-table plumbing
//! in exactly the sense `apply_changed_preference()` is, not because it is thematically
//! closer to route-walking than to any other applier.
//!
//! [`ApplyStep::Accent`](garage_core::schema::routes::ApplyStep::Accent) has no dedicated
//! module in this crate's map: the Python's `apply_accent()` is `render_accent()` (the file
//! half, ported to [`garage_render::accent`]) followed by `push_accent()` (the `gsettings`
//! push). Both are two or three lines each, and neither is named as its own file in the
//! approved module split, so their apply-side stub is kept here rather than inventing a file
//! the plan does not call for.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Render everything, then move the whole running session onto it. The session-start path.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub fn apply_preferences(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_preferences"))
}

/// Run a route step's command, and fail with its message if the command fails.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub. The real
/// implementation takes the command and message carried on
/// [`ApplyStep::RunOrRaise`](garage_core::schema::routes::ApplyStep::RunOrRaise) rather than
/// this fixed signature.
pub(crate) fn run_or_raise(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("run_or_raise"))
}

/// Publish the accent marker, then push it into GNOME's interface settings. See the module
/// doc for why this orphan step lives here.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_accent(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_accent"))
}
