//! `keybindings_snapshot()`: the shortcut list the pane draws, grouped the way `binds.lua` is
//! written.
//!
//! Resolves the published catalog against the user's document -- see
//! [`crate::keybind::catalog`] for the fail-closed witness-line contract that governs whether
//! an override can be trusted against it -- and groups the result by the catalog's own group
//! names, in first-appearance order, so the pane's sections mirror `binds.lua`'s own
//! organisation rather than an alphabetical resort. Custom shortcuts are reported separately
//! from the grouped defaults, deep-copied so the pane's own mutation of the response cannot
//! reach back into the loaded document.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.
