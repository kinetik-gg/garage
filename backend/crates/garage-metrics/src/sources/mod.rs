//! One module per hardware source, each owning the `/proc` and `/sys` paths for it.
//!
//! Split by device rather than by panel because stream and one-shot modes read the same
//! complete machine state and share every discovery rule.

pub(crate) mod cpu;
pub(crate) mod disk;
pub(crate) mod gpu;
pub(crate) mod memory;
pub(crate) mod net;
pub(crate) mod temp;
