//! The workspace block allocator: which display owns which range of ids, remembered rather
//! than recomputed.
//!
//! Every display owns a block of `WORKSPACE_BLOCK` ids -- 1-10, 11-20, 21-30 -- and its
//! workspace count decides how many slots at the front of that block are persistent. So
//! 8/4/4 is 1-8, 11-14, 21-24, and lowering the first display's count to 6 is 1-6 with the
//! other two untouched. The block is as wide as the count may ever be, which is what makes it
//! a fixed allocation rather than a reservation that could run out: a display cannot ask for
//! an eleventh workspace, because there is no eleventh number key to reach it with.
//!
//! Packing the ranges instead, each display starting where the previous one stopped, was
//! tried and is wrong: Hyprland ids are global and a window lives on the id, so renumbering a
//! range dragged its windows onto another monitor -- changing one display's count visibly
//! rearranged the whole desktop. Gaps in the numbering cost nothing next to that: no key and
//! no navigation path addresses a global id directly, and an id in a gap is simply a
//! workspace that does not exist.
//!
//! # Nothing is ever reclaimed
//!
//! A display unplugged in the morning and plugged back in at night finds its workspaces
//! where it left them, which is worth more than keeping the numbers small -- and reclaiming
//! would eventually hand a returning display's block to a newcomer, which is precisely the
//! collision blocks exist to avoid. Deriving a block from a display's position in the
//! ordering has the same bug one step away: unplugging the second of three displays would
//! slide the third down a block and take its windows with it, the same renumbering triggered
//! by a cable rather than by a count. So a display keeps its block for as long as the
//! allocator file remembers it, and one seen for the first time takes the lowest block
//! nothing else holds -- adding a display therefore never disturbs an existing one.
//!
//! Keyed by connector, as the counts and the workspace rules already are. Moving a display to
//! another port renames it, and it starts again with a block of its own; `displays.toml`
//! carries a description that would survive the move, but one identity for a display beats a
//! second one only this allocator would understand.
//!
//! # The one sanctioned layer-2 write
//!
//! Renderers write layer 3 and never layer 2, with exactly one exception, and it is this
//! allocator. [`RenderCx`](crate::cx::RenderCx)'s own doc names it: a pure `garage render`
//! can write `workspace-blocks.toml` because the allocation has to survive the render that
//! produced it -- unremembered, it would be recomputed from the current display ordering,
//! and then unplugging display 2 of 3 slides display 3 into block 2 and drags its windows
//! with it. There is one allocator, and reading it means running it.
//!
//! That write does not weaken the no-lock invariant `render_idle()` depends on:
//! `workspace-blocks.toml` is one of the four host files precisely because it is *not*
//! `preferences.toml` -- this allocator is its single writer, that single-writer discipline
//! is what serialises it, and `PREFERENCES_LOCK` is not involved on either side. The
//! invariant is about the lock, not about the layer.

use std::fmt::Write as _;
use std::path::Path;

use garage_core::fs::atomic::atomic_write;
use garage_core::schema::newtypes::{BlockId, ConnectorName, WORKSPACE_BLOCK};
use garage_core::toml_emit::{toml_value, Value};

use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::workspaces::WorkspaceGroup;

/// `load_toml()` (garage:1481-1489): an absent file is an empty document, and anything else
/// that goes wrong is `SettingsError(f"{path.name}: {error}")`.
///
/// `pub(crate)` rather than private to this module: [`crate::all::render_all`] reads
/// `displays.toml` through the same function, for the same reason `load_display_config()`
/// does in the Python -- both are "what does the file on disk say", not the lenient,
/// error-swallowing read [`load_saved_layout`] wraps it in below.
pub(crate) fn load_toml(path: &Path) -> Result<toml::Table, RenderError> {
    let name = || {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(toml::Table::new()),
        Err(error) => {
            return Err(RenderError::Toml {
                name: name(),
                detail: error.to_string(),
            })
        }
    };
    text.parse::<toml::Table>()
        .map_err(|error| RenderError::Toml {
            name: name(),
            detail: error.to_string(),
        })
}

/// The saved display layout, as much of it as the workspace plan reads.
///
/// `load_display_config()` (garage:2417-2423) plus the lenient half of `mirror_targets()`
/// (garage:2425-2453), narrowed to the one question asked here: which outputs own a
/// workspace range. A display that is disabled owns nothing, and neither does one that is a
/// mirror -- Hyprland pulls a mirrored output out of the monitor layout entirely.
///
/// Every failure the Python raises `SettingsError` for is `None` here, because both of
/// `workspace_outputs()`'s reads of this sit inside `try/except SettingsError`: an
/// unreadable, unparseable, or wrongly-shaped `displays.toml` contributes nothing rather
/// than taking the plan down.
struct SavedLayout {
    primary: String,
    owners: Vec<String>,
}

fn load_saved_layout(path: &Path) -> Option<SavedLayout> {
    let raw = load_toml(path).ok()?;
    // `raw.get("display", [])` with the "must be an array of tables" refusal: a missing key
    // is an empty layout, and a key of the wrong shape is the SettingsError both callers
    // swallow.
    let displays: &[toml::Value] = match raw.get("display") {
        None => &[],
        Some(value) => value.as_array()?.as_slice(),
    };
    let entries: Vec<(String, bool, String)> = displays
        .iter()
        .map(|item| {
            (
                py_str(item.get("output")),
                // `item.get("enabled", True)`, and Python's truthiness at that: only an
                // explicit false-y value turns a display off.
                item.get("enabled").is_none_or(py_truthy),
                // `str(item.get("mirror") or "")`.
                py_str_or_empty(item.get("mirror")),
            )
        })
        .collect();
    // mirror_targets(), lenient: a display may only mirror an enabled display that is not
    // itself a mirror, and anything else simply mirrors nothing. `enabled` is the Python's
    // dict comprehension, so a repeated output keeps its first position and its last mirror.
    let mut enabled: Vec<(String, String)> = Vec::new();
    for (output, _, source) in entries.iter().filter(|(_, on, _)| *on) {
        match enabled.iter_mut().find(|(name, _)| name == output) {
            Some(existing) => existing.1.clone_from(source),
            None => enabled.push((output.clone(), source.clone())),
        }
    }
    let mirrors: Vec<&str> = enabled
        .iter()
        .filter(|(output, source)| {
            !source.is_empty()
                && source != output
                && enabled
                    .iter()
                    .any(|(name, theirs)| name == source && theirs.is_empty())
        })
        .map(|(output, _)| output.as_str())
        .collect();
    Some(SavedLayout {
        primary: py_str(raw.get("primary")),
        // The Python walks `saved["displays"]` here rather than the mirror map, so a
        // repeated output really does appear twice in the ordering -- and then twice in the
        // plan. Faithful rather than tidied: nothing downstream depends on uniqueness, and
        // inventing a de-duplication would be a behaviour this port added.
        owners: entries
            .iter()
            .filter(|(output, on, _)| *on && !mirrors.contains(&output.as_str()))
            .map(|(output, _, _)| output.clone())
            .collect(),
    })
}

/// `str(value)` for the shapes `displays.toml` can hold here, and `""` for an absent key --
/// which is `item.get("output", "")` and `raw.get("primary", "")`.
fn py_str(value: Option<&toml::Value>) -> String {
    value.map_or_else(String::new, garage_core::schema::notes::py_str_toml)
}

/// `str(value or "")`: a false-y value is the empty string rather than its spelling.
fn py_str_or_empty(value: Option<&toml::Value>) -> String {
    match value {
        Some(inner) if py_truthy(inner) => py_str(value),
        _ => String::new(),
    }
}

/// Python's truthiness, for the two keys read through it here.
fn py_truthy(value: &toml::Value) -> bool {
    match value {
        toml::Value::Boolean(flag) => *flag,
        toml::Value::Integer(number) => *number != 0,
        toml::Value::Float(number) => *number != 0.0,
        toml::Value::String(text) => !text.is_empty(),
        toml::Value::Array(items) => !items.is_empty(),
        toml::Value::Table(table) => !table.is_empty(),
        // A datetime is an object, and every object is truthy.
        toml::Value::Datetime(_) => true,
    }
}

/// The displays that own a workspace range, in allocation order.
///
/// The saved layout leads, not the live monitor list. `DisplayPort` drops the connector when a
/// display sleeps, so `hyprctl monitors` stops reporting it -- and deriving the plan from
/// that would renumber every range and strand the windows on the ranges that disappeared,
/// every time a screen powered down. `displays.toml` records which displays the user
/// configured, which is what the plan should follow; a disabled or mirrored one still owns
/// nothing.
///
/// Pinning a range to a display that is momentarily absent is the milder failure: Hyprland
/// parks those workspaces on another output and hands them back on reconnect, windows intact.
///
/// Live monitors are folded in on top so a display the saved layout has never seen still gets
/// a range, and are the sole source if the layout cannot be read at all.
///
/// The primary comes first and the rest sort by connector name, which is stable across
/// sessions and needs nothing remembered to reproduce. Not by position: dragging a display
/// around in the Displays pane would otherwise renumber every workspace on the desktop.
///
/// `primary_environment` is `HYPR_PRIMARY_MONITOR`, read by the caller rather than here so
/// that a test can name it without reaching for the process environment.
#[must_use]
pub fn workspace_outputs(cx: &RenderCx<'_>, primary_environment: &str) -> Vec<ConnectorName> {
    let saved = load_saved_layout(&cx.paths().host.displays);
    let mut names: Vec<String> = saved
        .as_ref()
        .map(|layout| layout.owners.clone())
        .unwrap_or_default();
    // A monitor the saved layout predates. Appending rather than replacing keeps the
    // existing ranges stable when something new is plugged in.
    for monitor in cx.monitors().monitors().unwrap_or_default() {
        // `monitor_names()` (garage:3539-3544) drops a record with no usable name itself.
        if !monitor.name.is_empty() && !names.contains(&monitor.name) {
            names.push(monitor.name);
        }
    }
    let primary = saved.map(|layout| layout.primary).unwrap_or_default();
    let primary = if primary.is_empty() {
        primary_environment
    } else {
        &primary
    };
    let mut ordered: Vec<String> = names.into_iter().filter(|name| !name.is_empty()).collect();
    ordered.sort_unstable();
    if let Some(at) = ordered.iter().position(|name| name == primary) {
        let first = ordered.remove(at);
        ordered.insert(0, first);
    }
    ordered.iter().map(|name| name.as_str().into()).collect()
}

/// The block each display has already been given, by connector.
///
/// Lenient in the same way `parse_workspace_counts()` is, and for a stronger reason: an
/// unreadable entry here would otherwise take the workspace plan -- and with it the number
/// keys -- down with it. A bad entry is dropped, and the display it named is simply given a
/// block again.
///
/// # Errors
///
/// [`RenderError::Toml`] if the file itself cannot be read or parsed. That is the one
/// failure the Python does *not* swallow: `load_workspace_blocks()` calls `load_toml()`
/// outside any `except SettingsError`, so a malformed allocator file stops the render rather
/// than silently handing every display a new block.
pub fn load_workspace_blocks(cx: &RenderCx<'_>) -> Result<Vec<(String, u32)>, RenderError> {
    let stored = load_toml(&cx.paths().host.workspace_blocks)?;
    let Some(table) = stored.get("block").and_then(toml::Value::as_table) else {
        return Ok(Vec::new());
    };
    Ok(table
        .iter()
        .filter_map(|(output, index)| match index {
            // `isinstance(index, int) and not isinstance(index, bool) and index >= 0`: a
            // bool is an int in Python and is excluded by name, which TOML's own types
            // already do here. A negative index is dropped rather than clamped.
            toml::Value::Integer(number) if *number >= 0 => u32::try_from(*number)
                .ok()
                .map(|index| (output.clone(), index)),
            toml::Value::Integer(_)
            | toml::Value::String(_)
            | toml::Value::Float(_)
            | toml::Value::Boolean(_)
            | toml::Value::Datetime(_)
            | toml::Value::Array(_)
            | toml::Value::Table(_) => None,
        })
        .collect())
}

/// Write the allocator file: which block of workspace ids each display owns.
///
/// # Errors
///
/// [`RenderError::Atomic`] if the file cannot be replaced.
pub fn save_workspace_blocks(
    cx: &RenderCx<'_>,
    blocks: &[(String, u32)],
) -> Result<(), RenderError> {
    let mut ordered: Vec<&(String, u32)> = blocks.iter().collect();
    ordered.sort_by_key(|(_, index)| *index);
    let mut lines = String::from(
        "# Generated by garage. Which block of workspace ids each\n\
         # display owns; a display keeps its block for as long as this file says so.\n\
         [block]\n",
    );
    for (output, index) in ordered {
        // `toml_value(output)`, which quotes with the JSON encoder rather than TOML's own
        // escaping -- the same key spelling the Python emits.
        let key = toml_value(&Value::Str(output.clone())).unwrap_or_default();
        let _ = writeln!(lines, "{key} = {index}");
    }
    atomic_write(&cx.paths().host.workspace_blocks, &lines)?;
    Ok(())
}

/// The block index each display owns, remembered rather than recomputed. See the module doc
/// for why nothing here is ever reclaimed.
///
/// # Errors
///
/// [`RenderError::Toml`] from the read, [`RenderError::Atomic`] from the write. The write
/// only happens when a display was seen for the first time.
pub fn workspace_blocks(
    cx: &RenderCx<'_>,
    outputs: &[ConnectorName],
) -> Result<Vec<(String, BlockId)>, RenderError> {
    let mut blocks = load_workspace_blocks(cx)?;
    let mut taken: Vec<u32> = blocks.iter().map(|(_, index)| *index).collect();
    let fresh: Vec<&ConnectorName> = outputs
        .iter()
        .filter(|output| !blocks.iter().any(|(name, _)| name == output.as_str()))
        .collect();
    for output in &fresh {
        let mut index = 0;
        while taken.contains(&index) {
            index += 1;
        }
        // `blocks[output] = index`, which is an assignment and not an append: a
        // `displays.toml` naming one output twice puts it in `fresh` twice, and the second
        // pass overwrites the first's entry rather than adding a second one -- leaving the
        // block it first took allocated to nobody. Faithful, and harmless: an orphaned block
        // is exactly what the allocator's refusal to reclaim produces anyway.
        match blocks.iter_mut().find(|(name, _)| name == output.as_str()) {
            Some(existing) => existing.1 = index,
            None => blocks.push((output.to_string(), index)),
        }
        taken.push(index);
    }
    if !fresh.is_empty() {
        save_workspace_blocks(cx, &blocks)?;
    }
    Ok(blocks
        .into_iter()
        .map(|(output, index)| (output, BlockId::new(index)))
        .collect())
}

/// Which workspace IDs each attached display owns.
///
/// Every display owns a block of `WORKSPACE_BLOCK` ids and its count decides how many slots
/// at the front of that block are persistent -- see the module doc for the whole of it, and
/// for why the ranges are not packed.
///
/// # Errors
///
/// Whatever [`workspace_blocks`] fails with.
pub fn per_display_groups(
    cx: &RenderCx<'_>,
    primary_environment: &str,
) -> Result<Vec<WorkspaceGroup>, RenderError> {
    let workspaces = &cx.prefs().workspaces;
    let counts = workspaces.counts.parsed();
    let default = u32::try_from(workspaces.default_count.get()).unwrap_or(WORKSPACE_BLOCK);
    let outputs = workspace_outputs(cx, primary_environment);
    let blocks = workspace_blocks(cx, &outputs)?;
    Ok(outputs
        .into_iter()
        .filter_map(|output| {
            let block = blocks
                .iter()
                .find(|(name, _)| name == output.as_str())
                .map(|(_, block)| *block)?;
            let count = counts
                .iter()
                .find(|(name, _)| name == output.as_str())
                .map_or(default, |(_, size)| *size);
            Some(WorkspaceGroup {
                monitor: output,
                first: block.first(),
                count,
            })
        })
        .collect())
}
