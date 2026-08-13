//! `region_snapshot()`: the three `LANG`s the pane needs to tell the truth about a locale
//! change.
//!
//! Three, because they genuinely differ after a change: the system one the override sits on
//! top of (`/etc/locale.conf`), the one the systemd user manager will hand the next
//! application (`systemctl --user show-environment`), and the one this process was started
//! with -- which, since this binary is a child of the shell, is the locale the running
//! session is actually in (`$LANG` from the process environment). A pane that only showed
//! one of the three could not explain why a just-applied locale has not reached the terminal
//! it is running in yet.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.
