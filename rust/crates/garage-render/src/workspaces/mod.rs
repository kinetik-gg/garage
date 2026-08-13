//! The workspace plan and the block allocator it is built from.
//!
//! See [`plan`] for `render_workspaces()` and [`blocks`] for the allocator's never-reclaimed
//! invariant and the one sanctioned layer-2 write it makes.

pub(crate) mod blocks;
pub(crate) mod plan;
