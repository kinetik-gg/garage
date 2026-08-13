//! `apply_terminal()`: push the chosen terminal into the running session.
//!
//! Renders the general markers first (terminal, browser, launcher), then seeds
//! `TERMINAL=` into the systemd user manager's environment -- uwsm starts applications as
//! systemd user units, so this is what makes `$TERMINAL` true for anything launched after the
//! change, though not for anything already running. Reloads the compositor last, because
//! `binds.lua` reads the terminal marker while the config is parsed rather than per keypress,
//! unlike the launcher marker which is read fresh on every press.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Push the resolved terminal choice into the running session.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_terminal(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_terminal"))
}
