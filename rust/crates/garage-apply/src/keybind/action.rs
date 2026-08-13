//! `keybind_action()`: apply one change (rebind, reset, reset-all, add, update, remove) to
//! the shortcut set.
//!
//! Under the preferences lock, like `set` -- this is another whole-file read-modify-write,
//! and the pane can fire a rebind while a slider elsewhere is still landing on
//! `preferences.toml`. A rebind back to a shortcut's own default is a removal, not an
//! override that happens to agree with the default: keeping it as an override would leave the
//! pane showing the shortcut as changed, and would pin it if the default itself ever moved in
//! a later release.
//!
//! `reload_keybinds()` is the signal half: reload, then prove the desktop still has keys.
//! Nothing else would notice an empty bind set landing, because Hyprland's own rescue only
//! engages at zero binds and `binds.lua` always registers far more than zero -- so after the
//! reload, `hyprctl binds -j` is read back, and an empty result throws the overrides away
//! (back to the tracked default set), reloads again, and only then reports the failure. A set
//! that came out wrong without this check would simply be a desktop where a key does nothing,
//! discovered whenever the user next reached for it.
//!
//! Doc-only: mutates a document and reloads the compositor, but its real signature takes an
//! operation name and a JSON payload rather than this crate's fixed
//! `(cx: &mut SessionCx<'_>) -> Result<(), ApplyError>` shape -- reached through its own
//! `action keybind.*` command dispatch, never through `Route::steps()`.
