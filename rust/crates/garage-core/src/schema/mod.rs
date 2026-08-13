//! The preference schema: enums, newtypes, the generated `Preferences` struct,
//! and the typed route table.
//!
//! One table declares every key ([`prefs`]), one macro turns it into types
//! ([`macros`]), and every judgement it needs is an ordinary function
//! somewhere below it: what a usable value is ([`coerce`]), what an unusable
//! one is put back onto ([`defaults`]), what that substitution is reported as
//! ([`notes`]), and where `set` goes afterwards ([`routes`]).

pub mod coerce;
pub mod defaults;
pub mod enums;
mod macros;
pub mod newtypes;
pub mod normalise;
pub mod notes;
pub mod prefs;
pub mod ranged;
pub mod routes;

pub use crate::schema::defaults::{Defaults, MissingDefault};
pub use crate::schema::notes::Notes;
pub use crate::schema::prefs::{ParseKeyError, PreferenceKey, Preferences, Section, SetError};
pub use crate::schema::routes::Route;
