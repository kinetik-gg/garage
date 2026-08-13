//! `display_test()` and `display_finish()`: apply a candidate layout provisionally, and
//! confirm or revert it.
//!
//! `initialize_display_config()` lives here too: the first render on a machine that has never
//! had a saved layout seeds `displays.toml` from what Hyprland currently reports -- the
//! arrangement the catch-all monitor rule produced -- so `workspace_outputs()` has a saved
//! layout to lead with from the very first session rather than losing a sleeping display's
//! block before the user ever opens the Displays pane. Never an overwrite: the file belongs
//! to the user from the moment it exists, and silent when there is nothing to record, since
//! `garage render` also runs where no compositor is answering.
//!
//! # The fifteen-second watchdog invariant
//!
//! `display_test()` applies a candidate layout at once -- so the user can see it -- but does
//! not write it to `displays.toml` until it is confirmed. A detached watchdog process is
//! spawned alongside the apply, carrying the same token the pending-transaction file is keyed
//! on, and if nobody confirms within fifteen seconds it reverts the layout to what was
//! running before the test, unattended. That is what makes it safe to test a layout that
//! might leave no working input device or no visible display at all: the machine heals itself
//! back to a known-good arrangement without anyone having to find a keyboard that still
//! works.
//!
//! The watchdog has to survive the process that started it -- `display_test()` returns a
//! token immediately, well before fifteen seconds have passed -- which is why it is spawned
//! detached (a new session, stdio discarded) rather than run to completion inline.
//! `display_finish()` is idempotent against a watchdog that fires after the user already
//! confirmed: both paths take the same lock, and whichever runs second finds the pending
//! transaction already cleared and does nothing.
//!
//! Doc-only: operates on a display-layout value and a token, not
//! `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx), and display testing
//! is its own top-level command trio rather than a [`Route`](garage_core::schema::routes::Route)
//! step.
