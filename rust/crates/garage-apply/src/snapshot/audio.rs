//! `audio_snapshot()`: every sink and source `pactl` reports, simplified to what the pane
//! shows.
//!
//! Reads `pactl -f json info`, `list sinks` and `list sources`, then reduces each device to a
//! name, a description, whether it is the default, mute state and a volume fraction. The
//! volume comes from the first channel's `value_percent` -- devices report per-channel
//! volumes and the pane draws one slider, so the first channel stands for all of them, which
//! is the same simplification a stereo balance control would otherwise need a second slider
//! to avoid.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.
