//! Display layout: loading and normalising `displays.toml`, applying a layout to the running
//! compositor, and the confirm-or-revert transaction around a layout test.
//!
//! [`config`] is the load/save/normalise half, [`apply`] is `apply_display_layout()`'s
//! geometry checks and the reload that follows them, and [`transaction`] is
//! `display_test()`/`display_finish()`'s fifteen-second confirm-or-revert window, including
//! the watchdog invariant that makes an unconfirmed layout self-heal. None of the three is a
//! [`Route`](garage_core::schema::routes::Route) step -- display testing is its own top-level
//! command trio (`display-test`, `display-confirm`, `display-revert`) -- so all three stay
//! doc-only.

mod apply;
mod config;
mod transaction;
