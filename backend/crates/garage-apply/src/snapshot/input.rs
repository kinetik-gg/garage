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
