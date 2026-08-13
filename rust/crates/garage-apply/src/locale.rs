//! `apply_locale()`: push the chosen locale as far into the running session as it reaches.
//!
//! Not far. `LANG` is read once, at startup, by the C library, and nothing re-reads it, so no
//! already-running program can be moved to another locale. What can be moved is what has yet
//! to start: uwsm runs applications as systemd user units, so seeding the user manager's
//! environment -- and the D-Bus activation environment beside it, for services started that
//! way -- means anything opened from here on comes up in the new locale. The shell, the bar
//! and every open window keep the old one until the next login, which is what the pane says
//! in as many words.
//!
//! An empty override resolves to the system locale rather than clearing `LANG` to nothing:
//! `unset-environment` is used for that case specifically, rather than setting an empty
//! value, so a downstream reader sees "no override" and not "an empty language".
//! `dbus-update-activation-environment` is best-effort and skipped when the binary is not on
//! the machine -- it is not a hard dependency of the setting, and the systemd half is the one
//! that matters.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Seed the resolved locale into the systemd user manager's environment.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_locale(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_locale"))
}
