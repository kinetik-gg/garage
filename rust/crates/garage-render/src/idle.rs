//! `render_idle()`: `hypridle.conf` from the lock preferences, and nothing else.
//!
//! Its own function because it is its own entry point: `garage render-idle` is
//! `hypridle.service`'s `ExecStartPre`, and hypridle reads exactly this one file -- the
//! generated config names no include and sources nothing, so there is nothing else that unit
//! could need. Narrow on purpose. It used to run the full render there, which meant
//! restarting hypridle to pick up a new lock timeout also re-themed every toolkit, reloaded
//! the compositor and pushed `gsettings`, from inside the `set lock.*` that asked for the
//! restart.
//!
//! # The invariant this crate exists to hold
//!
//! Nothing here takes `PREFERENCES_LOCK`, and nothing here may ever be made to. `set lock.*`
//! holds that lock across a *synchronous* `systemctl restart hypridle.service`, whose
//! `ExecStartPre` re-enters this binary and runs this function: waiting on the lock here
//! would deadlock the setting against itself. [`crate::cx::RenderCx`]'s doc names this exact
//! call chain as invariant 4 -- the reason `garage-render` does not depend on `garage-prefs`
//! at all, so the lock this paragraph warns about cannot be reached from this function even
//! by accident.

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write `hypridle.conf` from `[lock]` alone.
///
/// # Errors
///
/// Always [`RenderError::PortPending`] until Phase 3 replaces this stub. The replacement
/// must uphold the no-lock invariant above; nothing about this signature enforces it beyond
/// what [`RenderCx`] already withholds.
pub(crate) fn render_idle(_cx: &RenderCx<'_>) -> Result<(), RenderError> {
    Err(RenderError::PortPending("render_idle"))
}
