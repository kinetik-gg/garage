//! What the Python backend does to a file, asserted against what this crate does to it.
//!
//! Every expectation in this module was generated during the Rust port by running the former
//! backend against a scratch `$HOME` holding the input beside it, and then written down here.
//! Nothing shells out at test time: the constant *is* the record of
//! what the Python said, in the same spirit as `garage-core`'s emitter fixtures.
//!
//! The cases are grouped by what they exercise -- [`load`] for the read path and its three
//! migrations, [`config_root`] for the v4 file move, [`save`] for the write path -- and a
//! handful of them are marked as divergences rather than parity: the Python crashes on three
//! hand-edited shapes (see [`load::divergences`]), and this port does not.

mod config_root;
mod load;
mod save;
