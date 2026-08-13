//! Lua-literal helpers shared between a generated fragment and a live `hyprctl eval`.
//!
//! See [`escape`] for quoting a string as a Lua literal, and [`emit`] for the table bodies
//! built from it. Both are doc-only: see either module for why.

pub(crate) mod emit;
pub(crate) mod escape;
