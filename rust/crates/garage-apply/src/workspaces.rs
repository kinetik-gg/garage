//! `apply_workspace_plan()`: move the session onto a new workspace plan without losing any
//! windows.
//!
//! Salvage first, reload second: the windows have to be moved while the workspaces they are
//! on are still the ones the compositor knows about, and while the workspace fragment has not
//! yet been overwritten -- it is the only record of the plan they were placed under. Remap
//! before reap, so a window is inside its own display's block before anything asks which
//! block it is in. Restoring what each display was showing runs last, because it needs the
//! surviving slots to already be back on their own displays before it sends anyone to one.
//!
//! # The support functions this orchestrates
//!
//! `reload_bar()` is the `pkill -USR2 waybar` signal every bar-owned change in this crate
//! shares -- named here because a workspace plan change is one of the things that can move
//! the bar's indicator, even though this function's own job stops at the compositor.
//!
//! `block_of()` answers which display's fixed block an id falls in, from the id alone, with
//! no reference to where the workspace happens to be sitting right now -- which matters while
//! a display sleeps, since `DisplayPort` drops the connector and Hyprland parks its workspaces
//! on another output, and asking the compositor would answer with whichever screen is
//! currently holding them.
//!
//! `surviving_slots()` answers two questions at once because they are the same question asked
//! twice: which ids the new plan keeps, and which slot a block's strays are sent to -- the
//! last slot of the block, not the first, because it is the end of the row that shrinks and
//! that is where the arrangement was interrupted.
//!
//! `windows_by_workspace()` reads every ordinary window's `(address, workspace id)` pair,
//! dropping special workspaces (negative ids, belonging to no block) so no count change may
//! ever disturb the scratchpad. `move_window()` salvages one window with
//! `movetoworkspacesilent` semantics -- `follow = false` -- because these are salvage moves,
//! not the user asking to go somewhere, so the focus stays where they left it.
//!
//! `installed_workspace_groups()` reads back the groups the fragment now on disk hands out,
//! because it is the only record of the plan the compositor is still running -- which is to
//! say, of the ids the windows are currently sitting on. Scraped with a pattern rather than
//! executed, since running it would be a second language in the loop for three integers.
//! Anything unreadable comes back empty, which makes `remap_workspaces()` a no-op: a plan
//! that cannot be compared is one nothing should be moved on.
//!
//! `remap_workspaces()` carries a display's windows across when its block of ids moves --
//! rare, but real: a display added to or dropped from the saved layout, or the allocation
//! itself changing shape. Addressed by window rather than by workspace, because the old and
//! new blocks can overlap and a workspace-by-workspace move would sweep windows just moved
//! into one along with the ones already there.
//!
//! `reap_stranded_windows()` rehomes windows left on a workspace the new plan no longer
//! keeps, before the reload that installs the new rules -- Hyprland does not destroy a
//! workspace that still has windows, so a lowered count leaves the surplus ones alive but off
//! the number keys until something moves them. Shared mode is a no-op for this and for
//! `restore_active_workspaces()` below: nothing is pinned in shared mode, so no window is
//! outside any display's surplus and no display's active workspace needs restoring against a
//! block that does not apply to it.
//!
//! `active_workspaces()` and `restore_active_workspaces()` are the read-then-write pair that
//! keeps a count change from silently moving what is on screen: a dropped slot, a remapped
//! slot, or an id handed to another display during the reload can all leave a monitor showing
//! nothing it asked for, so each display is sent back to what it was showing, translated
//! through the same remap its windows took, and falling back to the last slot it kept when
//! that workspace is gone for good.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Move the running session onto a new workspace plan, salvaging every window.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_workspace_plan(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_workspace_plan"))
}
