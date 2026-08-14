//! The one process boundary: every subprocess spawn in Garage runs through here.
//!
//! Three implementations, one per capability [`garage_core::traits`] names, and the split
//! between them is the split the Python draws between a *question* and an *instruction*:
//!
//! * [`System`] is [`Runner`](garage_core::traits::Runner) -- the general ability to run a
//!   program, which is the ability to move the running desktop. Only the apply side is
//!   handed one.
//! * [`Luac`] is [`LuaSyntaxCheck`](garage_core::traits::LuaSyntaxCheck) and [`Hyprctl`] is
//!   [`MonitorSource`](garage_core::traits::MonitorSource): the two named questions a
//!   render may ask. Each wraps a runner rather than reaching for one itself, so a caller
//!   that wants to see what a render asked can hand all three the same one.
//!
//! Nothing here is reachable from `garage-render`: that crate does not depend on this one,
//! and the `workspace_shape` cargo integration test fails the build if the edge ever appears.
#![forbid(unsafe_code)]

pub mod lua;
pub mod monitors;
pub mod run;

pub use lua::Luac;
pub use monitors::Hyprctl;
pub use run::{which, System};
