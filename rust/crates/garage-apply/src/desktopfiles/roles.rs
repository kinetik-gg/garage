//! `role_applications()`, `terminal_candidates()`, `resolve_terminal()`,
//! `terminal_command()`, `browser_command()` and `set_default_app()`: the per-role picker the
//! Default Applications pane draws from.
//!
//! `role_applications()` requires a candidate to register for *every* mimetype a role owns,
//! because selecting it writes every one of them at once -- the union of each mimetype's own
//! registrants would offer a text editor as a web browser, since editors commonly claim
//! `text/html` too, and then hand it a `https:` link it cannot open. Candidates are
//! deduplicated by display name rather than by desktop id, because one application can ship
//! two desktop files for itself (Chrome installs both `com.google.Chrome.desktop` and
//! `google-chrome.desktop`), and a combo box offering "Google Chrome" twice would give no way
//! to tell the two apart; whichever id is already in force keeps its name in the list, so the
//! current choice is always present in the offered set. `NoDisplay` entries are dropped
//! everywhere in this module for the same reason: an application saying it is not something
//! to pick (kitty's URL handler registers for `inode/directory` but is no file manager)
//! belongs in no menu and no combo.
//!
//! `terminal_candidates()` has no mimetype to search by -- there is no "a terminal" mimetype
//! -- so it is the one picker that searches the freedesktop `TerminalEmulator` category
//! across every desktop file directly instead.
//!
//! `browser_command()` resolves `$BROWSER`'s replacement for a bare keybinding: `$BROWSER` is
//! `xdg-open`, which is right for anything handing it a URL and useless as a keybind target,
//! since it exits having been given nothing to open. The bind is given the browser's own
//! `Exec=` line instead, resolved from whatever is currently registered rather than stored a
//! second time as its own preference.
//!
//! `set_default_app()` writes the association through
//! [`crate::desktopfiles::mime`]'s `write_mime_defaults()`, and additionally republishes the
//! browser marker and reloads the compositor when the role is `browser` specifically --
//! `binds.lua` reads that marker for the browser keybind the same way it reads the terminal
//! one.
//!
//! Doc-only: every function here reads installed applications or writes a mime association,
//! not `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx); reached from
//! `action defaults.*` and from [`crate::terminal`] and [`crate::snapshot::apps`], not from
//! `Route::steps()` directly.
