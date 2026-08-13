//! Preferences I/O and the lock it is serialised by.
//!
//! Layer 2 -- the user's own `preferences.toml` and its neighbours -- is read whole,
//! changed in memory and written whole. That shape makes every change a read-modify-write,
//! and the desktop runs several of them at once: the UI's sliders fire one `garage set` per
//! wheel notch. [`PrefLock`] is what serialises them, and it is the first thing here
//! because every function this crate will grow either holds it or is called by something
//! that does.
//!
//! Phase 3 brings the rest: loading the three layers into an effective configuration,
//! validating it, and saving the departures back. Until then this crate is the lock and its
//! documented invariants -- which are the part of the Python most easily lost in a port,
//! since taking the preferences lock on the load path deadlocks `garage set lock.*` against
//! its own synchronous `hypridle` restart. See [`lock`] for the full chain.
#![forbid(unsafe_code)]

pub mod lock;

pub use lock::{LockError, PrefLock};
