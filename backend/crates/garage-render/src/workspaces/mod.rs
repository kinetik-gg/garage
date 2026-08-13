//! The workspace plan and the block allocator it is built from.
//!
//! See [`plan`] for `render_workspaces()` and [`blocks`] for the allocator's never-reclaimed
//! invariant and the one sanctioned layer-2 write it makes.
//!
//! The two types below are the plan itself, which is the only thing this module hands
//! outwards: `garage-apply`'s `apply_workspace_plan()` asks for one, compares it against the
//! groups the installed fragment still hands out, and moves windows across the difference.
//! They are the Python's `{"mode": ..., "groups": [{"monitor", "first", "count"}]}` dicts,
//! given names and the newtypes that were introduced for exactly this.

pub mod blocks;
pub mod plan;

use garage_core::schema::enums::WorkspaceMode;
use garage_core::schema::newtypes::{ConnectorName, WorkspaceId};

/// One display's slice of the plan: which output owns it, where its block starts, and how
/// many slots at the front of that block are persistent.
///
/// [`monitor`](WorkspaceGroup::monitor) is empty in shared mode, which is what tells the Lua
/// side to leave `monitor` off the rule and address workspaces by id. A [`ConnectorName`]
/// carries that empty string as happily as a real connector: every string a compositor
/// reports is a valid connector, so there is nothing here to reject.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WorkspaceGroup {
    /// The connector this group is pinned to, or empty for the one shared group.
    pub monitor: ConnectorName,
    /// The first id in the group -- `block * WORKSPACE_BLOCK + 1` in per-display mode, and
    /// 1 in shared mode.
    pub first: WorkspaceId,
    /// How many slots from [`first`](WorkspaceGroup::first) are persistent. Never above
    /// `WORKSPACE_COUNT_MAX`, because there are only ten number keys to reach them with.
    pub count: u32,
}

/// The whole plan: which shape it is, and the groups it hands out.
///
/// Shared mode always carries exactly one group. Per-display mode carries one per display
/// the saved layout or the live compositor knows about, and may carry none at all -- nothing
/// detected and nothing saved -- which is the case `render_workspaces()` answers by removing
/// the fragment rather than writing an empty one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WorkspacePlan {
    /// `per-display` or `shared`, straight off `workspaces.mode`.
    pub mode: WorkspaceMode,
    /// The groups, in allocation order: the primary first, then by connector name.
    pub groups: Vec<WorkspaceGroup>,
}
