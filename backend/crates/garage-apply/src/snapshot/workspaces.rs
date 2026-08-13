//! `workspaces_snapshot()`: the allocation the pane draws, for the displays now attached.
//!
//! Resolved here rather than in QML, so the pane and the generated fragment cannot disagree
//! about which display owns which ids: there is one allocator (see
//! [`garage_render::workspaces::blocks`]), and this is a read of it. The per-display groups
//! are reported even in shared mode, so the pane can show what switching back to per-display
//! would give without the user having to switch modes to find out.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.

use garage_render::displays::{DisplayEntry, LayoutValue};
use garage_render::workspaces::blocks::per_display_groups;
use serde_json::{json, Value};

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// `WORKSPACE_COUNT_MAX` (garage:843): there are only ten number keys to reach them with.
const WORKSPACE_COUNT_MAX: u32 = 10;

/// `workspaces_snapshot()` (garage:5018-5037): the allocation the pane draws.
///
/// # Errors
///
/// [`ApplyError::Render`] if the block allocator could not read or write
/// `workspace-blocks.toml` -- which it may well write here, on a machine seeing a display for
/// the first time. That is the documented wrinkle of a snapshot that is otherwise a pure
/// read: the allocation is only stable because it is recorded, and the first sight of a
/// display is when it is recorded.
pub(crate) fn workspaces_snapshot(
    cx: &SessionCx<'_>,
    displays: &[DisplayEntry],
    primary_environment: &str,
) -> Result<Value, ApplyError> {
    let groups = per_display_groups(cx.render(), primary_environment)?;
    let rows: Vec<Value> = groups
        .iter()
        .map(|group| {
            let output = group.monitor.as_str();
            json!({
                "output": output,
                "label": label_for(displays, output),
                "count": group.count,
                "first": group.first.get(),
                "last": group.first.get() + group.count - 1,
            })
        })
        .collect();
    Ok(json!({ "max": WORKSPACE_COUNT_MAX, "displays": rows }))
}

/// `labels.get(group["monitor"], group["monitor"])` over
/// `{str(item["output"]): str(item.get("model") or item["output"])}`.
///
/// `or`, not a default: a display reporting an empty `model` -- which a laptop panel commonly
/// does -- is labelled by its connector rather than by nothing at all.
fn label_for(displays: &[DisplayEntry], output: &str) -> String {
    displays
        .iter()
        .find(|entry| entry.output() == output)
        .map_or_else(
            || output.to_owned(),
            |entry| {
                let model = entry
                    .get("model")
                    .map_or_else(String::new, LayoutValue::py_str);
                if model.is_empty() {
                    entry.output()
                } else {
                    model
                }
            },
        )
}
