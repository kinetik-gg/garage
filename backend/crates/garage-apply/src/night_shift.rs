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

use garage_render::theme::night_shift_active;

use crate::command::run;
use crate::cx::SessionCx;

/// Put the current night shift schedule into hyprsunset. `true` if it took
/// (garage:4797-4811).
///
/// Reports a `bool` rather than a `Result`, which is the Python's own signature: the call is
/// answered by hyprsunset over Hyprland's IPC and fails when hyprsunset is not up yet --
/// exactly the case at session start, where `autostart.lua` runs `garage apply` without
/// waiting for it. `apply_preferences()` drops the answer; `night-shift-sync` is the one
/// caller that can do something about it, and reports it in its own response.
pub fn apply_night_shift(cx: &mut SessionCx<'_>) -> bool {
    let prefs = cx.render().prefs();
    let result = if night_shift_active(prefs) {
        let temperature = prefs.appearance.night_shift_temperature.get().to_string();
        run(cx, &["hyprctl", "hyprsunset", "temperature", &temperature])
    } else {
        run(cx, &["hyprctl", "hyprsunset", "identity"])
    };
    result.status == 0
}
