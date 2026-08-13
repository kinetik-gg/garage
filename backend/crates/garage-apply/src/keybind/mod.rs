//! Keyboard shortcuts: parsing a combination, reading the published catalog, loading and
//! saving the user's document, guarding a candidate set, and acting on one change.
//!
//! [`parse`] turns a combination string into modifiers and a key, and derives the
//! id/canonical/signature spellings from it. [`catalog`] reads the published default set and
//! proves whether it can be trusted -- see its own doc for the fail-closed witness-line
//! contract. [`store`] loads and serialises `keybindings.toml`. [`guard`] refuses a shortcut
//! set that could not be undone from the desktop itself. [`action`] is `keybind_action()`,
//! the whole-file read-modify-write every rebind, reset or custom-shortcut edit goes through.
//!
//! None of these is a [`Route`](garage_core::schema::routes::Route) step -- keyboard shortcut
//! changes reach the session through their own `action keybind.*` command, not through
//! `apply_changed_preference()` -- so every submodule here stays doc-only.

mod action;
mod catalog;
mod guard;
mod parse;
mod store;
