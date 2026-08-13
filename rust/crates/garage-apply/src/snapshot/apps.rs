//! `default_apps_snapshot()`: the handler in force and the candidates, for every role the
//! pane offers.
//!
//! Read live rather than stored: the mime defaults belong to `mimeapps.list`, and a second
//! copy in `preferences.toml` would be nothing but a copy to drift. The terminal is the one
//! exception -- nothing on the system records a default terminal -- so it is a real
//! preference, resolved here against what is actually installed rather than read from a mime
//! association. See [`crate::desktopfiles::roles`] for `role_applications()` and
//! `resolve_terminal()`, which this composes per role.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.
