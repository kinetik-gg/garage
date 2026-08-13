//! `guard_keybinds()`: refuse a shortcut set that could not be undone from the desktop
//! itself.
//!
//! Checked before installing, not after. Hyprland's own emergency binds only appear once the
//! bind count reaches zero, which cannot happen while `binds.lua` registers a hundred of
//! them, so a set that merely lost its terminal shortcut would go entirely uncaught by that
//! mechanism. The rescue binds are already structurally safe on the Lua side -- `bind()`
//! never consults an override for them -- and this is the second lock on the same door,
//! positioned so the pane can explain why a change is refused rather than the user finding
//! out by pressing the key and getting nothing.
//!
//! Three checks, in order: the catalog must be non-empty (otherwise nothing has been
//! published yet), at least one rescue shortcut must exist in it (otherwise there is no way
//! back to a terminal even in principle), and every combination in the resolved set must be
//! unique -- every match on a combination fires, since `addKeybind` appends and the dispatch
//! path runs all of them, so two binds sharing one combination is not "the later one wins",
//! it is both actions on every press. Finally, every rescue bind's combination must still
//! resolve to that rescue bind and no other.
//!
//! Whether the catalog can be trusted changes what an unrecognised id is *called*, never
//! whether it is refused: against an unverified catalog (see
//! [`crate::keybind::catalog`]'s witness-line contract) an override naming an unknown id is
//! reported as "the shortcut list is still being published" rather than "there is no shortcut
//! called this" -- the truth when the catalog cannot yet be trusted is that publication is
//! mid-flight, not that the user's chosen shortcut has stopped existing.
//!
//! Doc-only: raises or returns nothing over a catalog/document pair, not
//! `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx).
