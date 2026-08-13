//! `doctor_report()`: the whole of `garage doctor --report` as one JSON-serializable object.
//!
//! This is the bug-reporting pipeline: one blob a user can paste, carrying the same checks
//! [`crate::doctor::checks`]' `doctor_checks()` walks -- so the printed report and this one
//! cannot drift apart -- plus the three facts a maintainer asks for first (generation
//! timestamp, checkout commit, Hyprland version) and the coercion notes `preferences.toml`
//! produced on the way past.
//!
//! # What is in it, and why none of it is a secret
//!
//! Everything here is either a version string, a path, a systemd unit name or a preference
//! value, and the preference values are the part that genuinely needed checking rather than
//! assuming: the notes carry the offending value verbatim, so the schema was read key by key.
//! Every one of its entries -- the version stamp and every setting across `appearance`,
//! `general`, `input`, `lock`, `region` and `workspaces` -- is a colour, a timeout, an enum
//! choice, a number, a wallpaper path, a locale, a terminal command or a search URL template.
//! There is no password, token, key or credential of any kind in the schema, because Garage
//! has nothing to authenticate to: no account, no sync, no remote. The closest thing to
//! free-form user text is the custom search URL template, and it is a template rather than a
//! secret.
//!
//! Nothing outside layer 2 is read into the report at all -- no environment, no journal, no
//! file contents beyond the preferences themselves. A key that could hold a credential must
//! not be added to the preference schema without revisiting this doc: the notes are printed
//! with the offending value in them, and that is the door a future key could open.
//!
//! Doc-only: assembles a JSON-serializable report value, not `Result<(), ApplyError>` over a
//! [`SessionCx`](crate::cx::SessionCx); reached from `doctor --report`'s own dispatch.
