//! Preferences I/O and the lock it is serialised by.
//!
//! Layer 2 -- the user's own `preferences.toml` and its neighbours -- is read whole,
//! changed in memory and written whole. That shape makes every change a read-modify-write,
//! and the desktop runs several of them at once: the UI's sliders fire one `garage set` per
//! wheel notch. [`PrefLock`] is what serialises them, and it is the first thing here
//! because every function this crate grew either holds it or is called by something
//! that does.
//!
//! # The read path
//!
//! [`load_preferences`] is the whole of it: layer 1 ([`shipped_defaults`]), then layer 2 off
//! the disk ([`load_toml`]), then [`migrate_preferences`], then
//! [`Preferences::coerce_from`](garage_core::schema::Preferences::coerce_from). Two of those
//! steps can write: the v5 compaction rewrites `preferences.toml` from the load path, under
//! a *non-blocking* acquire of [`PrefLock`] that skips its work when the lock is held. That
//! skip is not an optimisation -- see [`PrefLock::try_acquire`] for the deadlock it avoids.
//!
//! # The write path
//!
//! [`save_preferences`] is the only writer, and it writes departures from layer 1 and
//! nothing else. Everything about *what* a departure is lives in [`doc`]: a stored value
//! that equals the shipped default is absent from the file, so setting a value back to the
//! default erases the delta rather than pinning it.
//!
//! # Notes
//!
//! A note about the stored file -- a value put back, a key dropped -- goes to stderr or into
//! a caller's sink, and the sink is threaded through the whole chain rather than returned at
//! the end, exactly as the Python threads its `sink` argument. The reason is stated on
//! [`report_preference_notes`]: the Python prints at the moment the note is produced, so a
//! path that reports a dropped key and then fails has already said so.
#![forbid(unsafe_code)]

pub mod config_root;
pub mod doc;
pub mod error;
pub mod load;
pub mod lock;
pub mod migrate;
pub mod pyvalue;
pub mod save;

#[cfg(test)]
mod parity;
#[cfg(test)]
mod testing;

pub use config_root::migrate_config_root;
pub use doc::{
    preference_deltas, preference_document, preference_sections, preferences_table,
    report_preference_notes, same_default,
};
pub use error::PrefsError;
pub use load::{load_preferences, load_toml, shipped_defaults};
pub use lock::{LockError, PrefLock};
pub use migrate::{
    compact_preferences_file, migrate_preference_values, migrate_preferences, schema_version,
    PREFERENCES_VERSION,
};
pub use pyvalue::{py_element_equal, py_equal, py_equal_table};
pub use save::save_preferences;
