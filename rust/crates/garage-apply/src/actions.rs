//! `action()`: run one one-shot action -- volume, mute, default sink/source, night shift
//! toggle, glass reset, a keybind edit, a default-application change, an immediate lock, or a
//! date/time change.
//!
//! A second dispatch table, deliberately separate from [`crate::route`]'s route walking: an
//! action is not a preference, has no key in `preferences.toml` and no
//! [`Route`](garage_core::schema::routes::Route) of its own. It exists for the things the
//! pane needs to do that are not "set a value and move the session onto it" -- most of these
//! either read from or write straight to the world (`wpctl`, `pactl`, `loginctl`) with
//! nothing to persist, or they delegate to a whole-file read-modify-write of their own
//! (`keybind.*` into [`crate::keybind::action`], `defaults.*` into
//! [`crate::desktopfiles::roles`]).
//!
//! `glass.reset` is the one action that *is* a read-modify-write under the preferences lock,
//! exactly like `set`: every `glass_*` key is walked back to its shipped default without a
//! list of its own to keep in step with the schema, because every material preference is
//! named with that prefix. It is safe to block on the lock here, unlike the load path, which
//! only ever tries it, because nothing this action does restarts a service whose
//! `ExecStartPre` re-enters this binary.
//!
//! `datetime.ntp` and `datetime.timezone` are spawned detached rather than run to completion:
//! neither reads a result, and both hand the actual work to `systemd-timedated` over the bus,
//! so waiting on them would stall the settings path for an answer nothing here uses.
//!
//! Doc-only: the real signature takes an action name and a JSON payload rather than this
//! crate's fixed `(cx: &mut SessionCx<'_>) -> Result<(), ApplyError>` shape, and is reached
//! through the `action` command's own dispatch, never through `Route::steps()`.
