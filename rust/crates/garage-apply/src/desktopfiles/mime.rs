//! `mime_handlers()` and `write_mime_defaults()`: reading and writing default application
//! associations.
//!
//! `mime_handlers()` shells out to `gio mime` rather than reading the mimeinfo caches
//! directly: `gio` resolves the whole XDG lookup -- desktop-prefixed lists, config
//! directories, then data directories -- exactly as the application that will actually open
//! the file does, which a hand-rolled cache reader could only approximate. Run under
//! `LC_ALL=C` so the section headers this parses by (`"Default application for"`,
//! `"Registered applications"`) cannot be translated out from under it on a non-English
//! system.
//!
//! # The `MIMEAPPS_OVERRIDE` stow rationale
//!
//! `write_mime_defaults()` writes only `[Default Applications]`, and only into a
//! desktop-prefixed override file, never into `~/.config/mimeapps.list` itself. That file is
//! a stow symlink into the dotfiles repo -- tracked, hand-curated, holding the associations
//! and removals the checkout ships -- and this generated override is what the XDG spec's
//! lookup order lets sit in front of it without editing it. Writing straight into the tracked
//! file would mean every default-application change through the pane shows up as a dirty
//! working tree in the dotfiles checkout, which is the same category of mistake `render_theme()`
//! avoids by writing only generated paths.
//!
//! Existing entries are read back and merged with the new assignments before the file is
//! rewritten whole, rather than only appending, so a role changed twice does not leave two
//! conflicting lines for the same mimetype.
//!
//! Doc-only: reads/writes a small `.list`-format file, not `Result<(), ApplyError>` over a
//! [`SessionCx`](crate::cx::SessionCx); [`crate::desktopfiles::roles`]'s `set_default_app()`
//! is what calls it.
