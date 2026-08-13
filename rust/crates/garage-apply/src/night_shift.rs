//! `apply_night_shift()`: put the current schedule into hyprsunset. Reports whether it took.
//!
//! The call is answered by hyprsunset over Hyprland's IPC, so it fails when hyprsunset is not
//! up yet -- which is exactly the case at session start, where `autostart.lua` runs `garage
//! apply` without waiting for it. The Python's failure used to be silent, and the setting
//! simply did not apply until the timer's next tick; reporting it back is what lets the one
//! caller that can do something about it -- `night-shift-sync`'s response -- say so.
//!
//! The real signature reports `bool`, not only success or failure of the call itself: this
//! stub's fixed `Result<(), ApplyError>` shape is a placeholder for that distinction, which
//! Phase 3 restores.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Put the current night shift schedule into hyprsunset.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_night_shift(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_night_shift"))
}
