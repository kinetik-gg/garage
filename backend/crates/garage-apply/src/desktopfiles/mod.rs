//! Desktop file resolution: reading `.desktop` entries, mime default handlers, and the
//! per-role application picker.
//!
//! [`entry`] finds and parses a `.desktop` file by hand rather than with a general-purpose
//! `.ini` parser -- an `Exec` value carries `%` and `;` that interpolation fights, and a
//! desktop file legitimately repeats a key in localised forms that an `.ini` reader would
//! reject as a duplicate. [`mime`] resolves and writes mime default associations, and carries
//! the `MIMEAPPS_OVERRIDE` stow rationale -- see its own doc. [`roles`] is the per-role
//! candidate picker: which application currently handles a role (browser, file manager,
//! terminal, ...) and which installed ones could take it over.
//!
//! None of these is a [`Route`](garage_core::schema::routes::Route) step -- a default
//! application change reaches the session through `action defaults.*`, not through
//! `apply_changed_preference()` -- so every submodule here stays doc-only.

// Every function in this subtree now has a real body, but none is reachable from outside
// `desktopfiles` yet: wiring `set_default_app()` and its neighbours into `action defaults.*`
// (dispatch), `crate::terminal` and `crate::snapshot::apps` is a later task's job, the same
// "a later task's stubs" position `workspaces::reload_bar` was in before its own callers
// existed. One allow here, rather than one per function, because the whole module is in that
// position together and each of the three submodules calls into the others.
#![allow(dead_code)]

mod entry;
mod mime;
mod roles;
