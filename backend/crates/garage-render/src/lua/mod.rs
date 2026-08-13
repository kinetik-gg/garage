//! Lua-literal helpers shared between a generated fragment and a live `hyprctl eval`.
//!
//! See [`escape`] for quoting a string as a Lua literal, and [`emit`] for the table bodies
//! built from it. Both are doc-only: see either module for why.

pub(crate) mod emit;
/// Public because a live `hyprctl eval` is built on the apply side: `move_window()` and
/// `restore_active_workspaces()` in `garage-apply` quote a workspace id, an address and a
/// connector with the same one function the generated fragments use. One quoting rule, one
/// implementation -- which is what this module's own first line already promised.
pub mod escape;
