//! `load_keybindings()`, `keybindings_toml()` and `custom_keybind()`: reading and writing the
//! user's shortcut document.
//!
//! `load_keybindings()` is lenient in the way `parse_workspace_counts()` is: this file is
//! small and obvious enough that someone will edit it by hand, and one bad line must not make
//! the whole document unloadable when the empty document is already the tracked default set --
//! the worst case is one shortcut quietly returning to what `binds.lua` gives it.
//!
//! Overrides are filtered against [`crate::keybind::catalog`]'s published set only when that
//! catalog is verified whole -- see its fail-closed witness-line contract -- because this
//! document is what a rebind renders back over `keybindings.toml`, and dropping an override
//! the reader could not vouch for would be a silent, permanent deletion of the user's choice
//! rather than a refusal the user could retry. An override naming a bind the verified catalog
//! genuinely no longer publishes is dropped and reported to stderr, the one case where the
//! user does lose something they chose from the pane and so the one worth a line in the
//! journal.
//!
//! `custom_keybind()` checks and normalises one user-defined shortcut. The command is never
//! inspected beyond its length and character set -- it is a shell command line, which is the
//! whole feature, and it reaches the compositor as data with nothing here for it to break out
//! of.
//!
//! Doc-only: reads and writes a document value, not `Result<(), ApplyError>` over a
//! [`SessionCx`](crate::cx::SessionCx); [`crate::keybind::action`] is what turns a change into
//! a write.
