//! Display layout: normalising and saving `displays.toml`, applying a layout to the running
//! compositor, and the confirm-or-revert transaction around a layout test.
//!
//! [`config`] is the normalise/serialise half, [`apply`] is `apply_display_layout()`'s
//! geometry checks and the reload that follows them, [`transaction`] is
//! `display_test()`/`display_finish()`'s fifteen-second confirm-or-revert window plus the
//! seeding that gives a fresh machine a `displays.toml` at all, and [`wire`] is the JSON
//! boundary -- the `display-test` payload, the pending-transaction file and
//! `hyprctl monitors all -j` all arrive as JSON, while the value everything here works on is
//! [`garage_render::displays::LayoutValue`].
//!
//! None of the four is a [`Route`](garage_core::schema::routes::Route) step: display testing
//! is its own top-level command trio -- `display-test`, `display-confirm`, `display-revert`,
//! plus the watchdog's unlisted re-entry point -- so nothing here appears in
//! [`crate::dispatch`].
//!
//! # Where the reader lives, and why not here
//!
//! The scaffold for this module said `load_display_config()` and `mirror_targets()` would sit
//! in [`config`] and that `garage_render::displays` would call them. That is impossible in the
//! direction it was written: `garage-apply` depends on `garage-render`, and the edge does not
//! run the other way. So the shared half -- the layout types,
//! [`load_display_config`](garage_render::displays::load_display_config) and
//! [`mirror_targets`](garage_render::displays::mirror_targets) -- lives in
//! [`garage_render::displays`], the lowest crate both halves can name, and everything here
//! consumes it. That is the same placement, for the same reason, as
//! [`garage_render::keybinds`]'s `Document`: the shape of a user-owned file sits beside the
//! renderer that consumes it, and every rule about what may go into it sits in the crate that
//! may act on the session.

pub mod apply;
pub mod config;
pub mod transaction;
pub(crate) mod wire;
