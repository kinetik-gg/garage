//! `datetime_snapshot()`: the timezone, NTP state and the full timezone list.
//!
//! The timezone falls back through three sources in order: `timedatectl show
//! --property=Timezone`, then `/etc/localtime`'s resolved target split on `/zoneinfo/`, then
//! `time.tzname[0]` as the last resort -- covering a machine with no `timedatectl`, one whose
//! `localtime` symlink is not the standard shape, and one with neither. The timezone list
//! itself prefers `timedatectl list-timezones` and falls back to Python's own
//! `available_timezones()` when that command fails, so the pane always has something to
//! populate its picker from.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.
