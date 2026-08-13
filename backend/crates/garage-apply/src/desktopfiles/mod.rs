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

mod entry;
mod mime;
mod roles;
