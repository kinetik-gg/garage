//! The salvage half: what moves a window, and what decides where it goes.
//!
//! Every function here is best-effort by design. Each is one `hyprctl` call whose failure the
//! Python swallows too, because stopping halfway through a salvage would leave the session
//! between two plans -- worse than an unmoved window. The one thing that may fail loudly is
//! the reload itself, and that lives with the orchestration in [`super`].
//!
//! Read [`super`]'s module doc first: it is where the order these run in, and the reason for
//! it, is written down.

use garage_core::schema::enums::WorkspaceMode;
use garage_core::schema::newtypes::WORKSPACE_BLOCK;
use garage_render::lua::escape::lua_string;
use garage_render::workspaces::WorkspacePlan;
use serde_json::Value;

use crate::command::{json_list, run};
use crate::cx::SessionCx;
use crate::workspaces::installed::installed_workspace_groups;

/// Which display's block an id falls in.
///
/// The whole point of fixed blocks: the id alone says who owns it, with no reference to where
/// the workspace happens to be sitting. That matters while a display sleeps -- `DisplayPort`
/// drops the connector and Hyprland parks its workspaces on another output -- and asking the
/// compositor would answer with whichever screen is holding them.
pub(crate) fn block_of(workspace: u32) -> u32 {
    workspace.saturating_sub(1) / WORKSPACE_BLOCK
}

/// The ids the plan keeps, and where each block's strays are sent.
///
/// One reader for both because they are the same question asked twice: what a display still
/// owns, and which of those a window goes to when the slot it was on is gone. The last slot,
/// not the first: it is the end of the row that shrinks, so that is where the arrangement was
/// interrupted.
pub(crate) fn surviving_slots(plan: &WorkspacePlan) -> (Vec<u32>, Vec<(u32, u32)>) {
    let mut keep = Vec::new();
    let mut last: Vec<(u32, u32)> = Vec::new();
    for group in &plan.groups {
        let first = group.first.get();
        for offset in 0..group.count {
            keep.push(first + offset);
        }
        let block = block_of(first);
        let end = first + group.count - 1;
        match last.iter_mut().find(|(index, _)| *index == block) {
            Some(existing) => existing.1 = end,
            None => last.push((block, end)),
        }
    }
    (keep, last)
}

/// Every ordinary window as `(address, workspace id)`.
///
/// Special workspaces carry negative ids and belong to no block, so the scratchpad is dropped
/// here rather than guarded against at each caller: no count change may disturb it.
pub(super) fn windows_by_workspace(cx: &SessionCx<'_>) -> Vec<(String, u32)> {
    let mut placed = Vec::new();
    for client in json_list(cx, &["hyprctl", "clients", "-j"]) {
        let Some(record) = client.as_object() else {
            continue;
        };
        let workspace = record.get("workspace").and_then(Value::as_object);
        let id = workspace
            .and_then(|table| table.get("id"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let Ok(id) = u32::try_from(id) else { continue };
        if id == 0 || workspace.is_none() {
            continue;
        }
        let address = record
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !address.is_empty() {
            placed.push((address.to_owned(), id));
        }
    }
    placed
}

/// Move one window, silently.
///
/// `follow = false` is `movetoworkspacesilent`: these are salvage moves, not the user asking
/// to go somewhere, so the focus stays where they left it.
fn move_window(cx: &SessionCx<'_>, address: &str, workspace: u32) {
    let body = format!(
        "hl.dispatch(hl.dsp.window.move({{ workspace = {}, follow = false, window = {} }}))",
        lua_string(&workspace.to_string()),
        lua_string(&format!("address:{address}"))
    );
    drop(run(cx, &["hyprctl", "eval", &body]));
}
/// Carry a display's windows across when its block of ids moves.
///
/// A block is fixed against the counts but not against the ordering it is indexed by, so it
/// does move on the rare occasions that ordering changes: a display added to or dropped from
/// the saved layout, and once when the allocation itself changed from packed ranges to fixed
/// blocks. Without this the ids under a window would be handed to another display and the
/// window would travel with them -- the very thing blocks exist to stop.
///
/// Slot by slot within the one display, so the arrangement survives intact: its nth workspace
/// stays its nth. Every slot the old plan allocated is mapped, including ones past the new
/// count, because a stale id left inside another display's new block would be reaped to that
/// display; sending it into its own block first is what keeps the reap below honest.
///
/// Addressed by window rather than by workspace because the old and new blocks overlap.
/// Moving workspace by workspace would sweep the windows just moved into one along with the
/// ones that were already there.
///
/// Returns the mapping so the focus can be made to follow it too.
pub(super) fn remap_workspaces(cx: &SessionCx<'_>, plan: &WorkspacePlan) -> Vec<(u32, u32)> {
    if plan.mode != WorkspaceMode::PerDisplay {
        return Vec::new();
    }
    let installed = installed_workspace_groups(cx);
    let mut moves: Vec<(u32, u32)> = Vec::new();
    for group in &plan.groups {
        // `{group["monitor"]: group for group in installed}`: a repeated monitor keeps its
        // last group, which is what the dict comprehension does.
        let Some((_, first, count)) = installed
            .iter()
            .rev()
            .find(|(monitor, _, _)| monitor == group.monitor.as_str())
        else {
            continue;
        };
        if *first == group.first.get() {
            continue;
        }
        for offset in 0..*count {
            let from = first.saturating_add(offset);
            let to = group.first.get() + offset;
            match moves.iter_mut().find(|(source, _)| *source == from) {
                Some(existing) => existing.1 = to,
                None => moves.push((from, to)),
            }
        }
    }
    if !moves.is_empty() {
        for (address, workspace) in windows_by_workspace(cx) {
            if let Some((_, target)) = moves.iter().find(|(source, _)| *source == workspace) {
                move_window(cx, &address, *target);
            }
        }
    }
    moves
}

/// Rehome windows left on a workspace the new plan no longer keeps.
///
/// Hyprland does not destroy a workspace that still has windows, so lowering a count leaves
/// the surplus ones alive but off the number keys and out of the monitor's cycle only once
/// they are empty. Rather than leave that to be discovered later, each window on one is moved
/// to the last slot its own display still keeps.
///
/// Which display that is comes from the id's block, not from the compositor. Reading it back
/// from `hyprctl workspaces` was wrong in exactly one case, and it is a case this machine
/// hits: while a display sleeps its workspaces are parked on another output, so a count
/// change made during the nap reported the sleeping display's windows as belonging to
/// whichever screen was holding them and moved them there for good.
///
/// Runs before the reload that installs the new rules. Afterwards the surplus workspaces are
/// empty, which is the one state Hyprland does clean up.
///
/// Shared mode is a no-op, which is the right answer for it: nothing is pinned, so a window
/// outside the pool is on no display's surplus. Its workspace keeps existing because it has
/// windows in it, and the adjacent workspace binds still reach it -- only the number keys
/// stop at the pool.
pub(super) fn reap_stranded_windows(cx: &SessionCx<'_>, plan: &WorkspacePlan) {
    if plan.mode != WorkspaceMode::PerDisplay {
        return;
    }
    let (keep, last) = surviving_slots(plan);
    if keep.is_empty() {
        return;
    }
    for (address, workspace) in windows_by_workspace(cx) {
        if keep.contains(&workspace) {
            continue;
        }
        // An id in no display's block belongs to no display, and any target for it would be
        // a guess at somebody else's monitor.
        if let Some((_, target)) = last.iter().find(|(block, _)| *block == block_of(workspace)) {
            move_window(cx, &address, *target);
        }
    }
}

/// What each display is showing, by connector.
pub(super) fn active_workspaces(cx: &SessionCx<'_>) -> Vec<(String, u32)> {
    let mut showing: Vec<(String, u32)> = Vec::new();
    for monitor in json_list(cx, &["hyprctl", "monitors", "-j"]) {
        let Some(record) = monitor.as_object() else {
            continue;
        };
        let current = record
            .get("activeWorkspace")
            .and_then(Value::as_object)
            .and_then(|active| active.get("id"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let Ok(current) = u32::try_from(current) else {
            continue;
        };
        if current > 0 {
            let name = record
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            match showing.iter_mut().find(|(seen, _)| *seen == name) {
                Some(existing) => existing.1 = current,
                None => showing.push((name, current)),
            }
        }
    }
    showing
}

/// The per-monitor half of [`restore_active_workspaces`]: which display had the focus, and
/// which workspace each display has to be sent back to.
///
/// Split out from its caller only for length. The loop is the Python's, unchanged: the
/// remembered workspace, translated through the remap, and the display's own last surviving
/// slot when that translation names an id the plan no longer keeps.
fn restore_targets(
    cx: &SessionCx<'_>,
    keep: &[u32],
    last: &[(&str, u32)],
    moves: &[(u32, u32)],
    showing: &[(String, u32)],
) -> (String, Vec<u32>) {
    let mut focused = String::new();
    let mut targets: Vec<u32> = Vec::new();
    for monitor in json_list(cx, &["hyprctl", "monitors", "-j"]) {
        let Some(record) = monitor.as_object() else {
            continue;
        };
        let name = record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if record.get("focused").is_some_and(truthy) {
            focused.clone_from(&name);
        }
        let current = record
            .get("activeWorkspace")
            .and_then(Value::as_object)
            .and_then(|active| active.get("id"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let current = u32::try_from(current).unwrap_or(0);
        // A display that arrived while this was running was showing nothing to restore, so
        // whatever it is on now is what it wanted.
        let previous = showing
            .iter()
            .find(|(seen, _)| *seen == name)
            .map_or(current, |(_, id)| *id);
        let mut target = moves
            .iter()
            .find(|(source, _)| *source == previous)
            .map_or(previous, |(_, to)| *to);
        if !keep.contains(&target) {
            let Some((_, fallback)) = last.iter().find(|(owner, _)| *owner == name) else {
                continue;
            };
            target = *fallback;
        }
        if target != current {
            targets.push(target);
        }
    }
    (focused, targets)
}

/// Put every display back on the workspace it was showing.
///
/// Changing a count should not change what is on screen, and three things conspire to change
/// it anyway. A dropped slot leaves a display on a workspace that is about to stop existing --
/// Hyprland collects an emptied workspace unless it is the one on screen, so it lingers with
/// no rule, no number key and none of its windows. A remapped slot takes its windows to a new
/// id and leaves the display on the old, empty one. And when the reload hands an id to
/// another display, Hyprland re-homes that whole workspace, leaving the display it left with
/// no active workspace at all and free to pick whichever of its own it likes.
///
/// So each display is sent back to what it was showing, translated through the same remap its
/// windows took, and falling back to the last slot it kept -- where the reap sent them --
/// when that workspace is gone for good.
///
/// Runs after the reload, when every surviving slot exists on its own display again;
/// switching to one beforehand could create it on the wrong screen. The target is chosen per
/// monitor rather than from the id's block, because a display can only be showing a workspace
/// while it is awake, so the connector it reports is unambiguous -- and unlike the block, it
/// is still right for an id that predates the current allocation. Switching a workspace also
/// focuses the monitor it is pinned to, so the display that had the focus is given it back.
pub(super) fn restore_active_workspaces(
    cx: &SessionCx<'_>,
    plan: &WorkspacePlan,
    moves: &[(u32, u32)],
    showing: &[(String, u32)],
) {
    if plan.mode != WorkspaceMode::PerDisplay {
        return;
    }
    let (keep, _) = surviving_slots(plan);
    if keep.is_empty() {
        return;
    }
    let mut last: Vec<(&str, u32)> = Vec::new();
    for group in &plan.groups {
        let end = group.first.get() + group.count - 1;
        match last
            .iter_mut()
            .find(|(name, _)| *name == group.monitor.as_str())
        {
            Some(existing) => existing.1 = end,
            None => last.push((group.monitor.as_str(), end)),
        }
    }
    let (focused, targets) = restore_targets(cx, &keep, &last, moves, showing);
    for target in &targets {
        let body = format!(
            "hl.dispatch(hl.dsp.focus({{ workspace = {} }}))",
            lua_string(&target.to_string())
        );
        drop(run(cx, &["hyprctl", "eval", &body]));
    }
    if !targets.is_empty() && !focused.is_empty() {
        let body = format!(
            "hl.dispatch(hl.dsp.focus({{ monitor = {} }}))",
            lua_string(&focused)
        );
        drop(run(cx, &["hyprctl", "eval", &body]));
    }
}

/// Python's truthiness for `monitor.get("focused")`.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|size| size != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(table) => !table.is_empty(),
    }
}
