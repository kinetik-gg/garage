//! `garage doctor`: a read-only health report for a person at a terminal, not the QML client.
//!
//! # Plumbing, deliberately unlike everything above it
//!
//! Every other command in this crate answers the QML client and prints one JSON object; this
//! one -- together with [`crate::repair`] and [`crate::update`] -- answers a person at a
//! terminal and prints lines, so nothing here reaches `make_snapshot()` and nothing here goes
//! through the `{"ok", "data", "error"}` response envelope. The CLI's command dispatch routes
//! these three ahead of the JSON path for exactly that reason. `doctor --report` is the one
//! partial exception: it prints JSON, because a bug report has to be pasted somewhere, but it
//! is still the person's command and still not the response envelope -- see [`report`].
//!
//! # A check is (label, probe)
//!
//! [`checks`] is the probe list: each probe returns a status (`ok` healthy, `note` true and
//! reported but not a problem -- a machine with no plugins deployed, a session that is not
//! running because this is a TTY -- or `fail`, a real problem whose hint says what to do
//! about it), a detail string, and a hint. Exit status ignores `note`, because a health check
//! that fails on a TTY is a health check nobody runs. The probes return their verdict rather
//! than printing it, so [`report`]'s `doctor --report` serializes the same list the printed
//! report walks rather than a second copy of it that could drift.
//!
//! [`stow`] is the stow-link and dangling-link half (`stow_state()`, `dangling_repo_links()`,
//! `managed_paths()`), and [`plugins`] is the Hyprland plugin ABI comparison, reimplemented
//! here rather than shelled out to `garage-rebuild-plugins --check` -- see its own doc for
//! why running that script's logic inline is the deliberate choice.
//!
//! Doc-only throughout: every function here takes `argv`/returns `(status, detail, hint)` or
//! an exit code, not `Result<(), ApplyError>` over a
//! [`SessionCx`](crate::cx::SessionCx).

mod checks;
mod plugins;
mod report;
mod stow;
