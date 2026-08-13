//! `input_snapshot()`: whether a touchpad is attached, for the pane to decide which sliders
//! to show.
//!
//! Reads `hyprctl devices -j` and answers true if either the touch device list is non-empty
//! or any mouse's reported name contains "touchpad" -- Hyprland reports a touchpad as a mouse
//! device with that word in its name rather than as a distinct class, so the name match is
//! the only signal there is.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.

use serde_json::{json, Value};

use crate::command::json_object;
use crate::cx::SessionCx;

/// `input_snapshot()` (garage:5066-5071): whether a touchpad is attached.
pub(crate) fn input_snapshot(cx: &SessionCx<'_>) -> Value {
    let devices = json_object(cx, &["hyprctl", "devices", "-j"]);
    let touch = devices
        .get("touch")
        .and_then(Value::as_array)
        .is_some_and(|list| !list.is_empty());
    // Hyprland reports a touchpad as a mouse device with the word in its name rather than as
    // a distinct class, so the name match is the only signal there is.
    let named = devices
        .get("mice")
        .and_then(Value::as_array)
        .is_some_and(|mice| {
            mice.iter().filter(|item| item.is_object()).any(|item| {
                item.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("touchpad")
            })
        });
    json!({ "hasTouchpad": touch || named })
}
