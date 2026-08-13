//! `garage repair`: the way back from a `preferences.toml` this build cannot parse.
//!
//! The one user file deliberately left read-only in that state. Everything else about layer
//! 2 heals itself: a value out of range is coerced with a note, an unknown key is dropped, a
//! withdrawn spelling is carried across. But a file that is not TOML at all has no values to
//! coerce, and every writer loads the file before it writes -- which means the settings pane
//! cannot correct the very file that is blocking it. That is deliberate (guessing at what a
//! half-typed file meant would be worse than refusing it) and it is what leaves the gap this
//! command fills.
//!
//! Only `preferences.toml`, never the other three user files: those are records rather than
//! settings, and each already has its own way back on its own -- an unconfirmed display
//! layout reverts through [`crate::displays::transaction`]'s watchdog, `binds.lua`'s rescue
//! shortcuts never consult `keybindings.toml` at all, and a workspace block that cannot be
//! read is simply handed out again by
//! [`garage_render::workspaces::blocks`]. None of them can lock the user out, so none of them
//! needs a command -- and a `repair` that quietly reset all four would take the user's
//! shortcuts away to fix their wallpaper.
//!
//! `repair_preview()` (no arguments) explains exactly what `--reset` would do and changes
//! nothing: a command that resets the user's settings does not get to do it because they
//! typed its name once. The first run is the explanation, the second run is the consent.
//! `repair_reset()` backs the file up under a name that is never reused -- `O_EXCL` in a loop
//! against a whole-second timestamp, so two repairs in the same second cannot clobber each
//! other's backup -- writes a fresh stamp-only file, and proves it loads, all under
//! `PREFERENCES_LOCK` since the swap is a read-modify-write like any other and `set` may be
//! running in the pane at the same moment.
//!
//! Doc-only: takes `argv` and returns an exit code, prints lines rather than the JSON
//! response envelope, and is dispatched ahead of the JSON command path the same way
//! [`crate::doctor`] and [`crate::update`] are.
