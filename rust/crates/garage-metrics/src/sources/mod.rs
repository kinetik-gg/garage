//! One module per hardware source, each owning the `/proc` and `/sys` paths for it.
//!
//! Split by device rather than by widget because the two `--stream` surfaces read every
//! source at once while `--bar-svg` reads exactly one -- see [`crate::modes`] for why the
//! second of those matters more than the first.

pub(crate) mod cpu;
pub(crate) mod disk;
pub(crate) mod gpu;
pub(crate) mod memory;
pub(crate) mod net;
pub(crate) mod temp;
