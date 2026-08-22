//! The palette itself, and every toolkit writer built from it.
//!
//! [`table`] and [`accents`] are the data; [`gtk`], [`rofi`], [`qt`] and [`swayosd`] are
//! the per-toolkit writers [`toolkits`]' `render_toolkits()` calls for one resolved
//! scheme. All doc-only -- see each module for why.

pub(crate) mod accents;
pub(crate) mod gtk;
#[cfg(test)]
mod parity;
pub(crate) mod qt;
pub(crate) mod rofi;
pub(crate) mod swayosd;
pub(crate) mod table;
pub(crate) mod toolkits;
