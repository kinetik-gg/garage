//! The three file writers. Which one a path gets is a load-bearing decision:
//! see each submodule's docs, ported from the Python they replace.

pub mod atomic;
pub mod lua;
pub mod marker;
