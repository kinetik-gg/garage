//! `apply_display_layout()`: check a candidate layout's geometry, then render and reload it.
//!
//! Every enabled, non-mirrored display's rectangle is checked against every other's: no two
//! may overlap, and the whole set must be edge-to-edge connected -- reachable from the first
//! display by crossing only touching edges -- or the layout would contain a gap no window
//! could ever be dragged across. A mirror is excluded from both checks entirely, because
//! Hyprland stacks it exactly on its source, which would otherwise read as a total overlap,
//! and it touches no edge of its own to connect through.
//!
//! Touching is judged with a half-pixel tolerance on each edge, which is what lets two
//! displays of different native scales still register as adjacent despite floating-point
//! rounding in their scaled widths and heights.
//!
//! Only once the geometry is accepted does this render the fragment and reload the
//! compositor -- checked before written, so a rejected layout never reaches `hyprland.lua`
//! at all.
//!
//! Doc-only in the dispatch sense: this operates on a display-layout value rather than being
//! a [`Route`](garage_core::schema::routes::Route) step, and is reached from
//! [`super::transaction`].

use garage_render::displays::{
    render_displays, strict_mirror_targets, DisplayEntry, DisplayLayout,
};

use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::workspaces::run;

/// The half-pixel slack every edge comparison is given. See the module doc.
const TOLERANCE: f64 = 0.5;

/// One display's rectangle on the desktop, in layout coordinates.
struct Rectangle {
    output: String,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

/// `apply_display_layout()` (garage:5177-5230): refuse a layout that does not hold together,
/// then put it on screen.
///
/// # Errors
///
/// [`ApplyError::Layout`] for an empty layout, one with nothing but mirrors, an overlap or a
/// gap; [`ApplyError::Mirror`] for a mirror Hyprland would ignore; [`ApplyError::Number`] for
/// a record whose geometry is not numbers; [`ApplyError::Render`] if the fragment cannot be
/// written; and [`ApplyError::Layout`] again if the compositor refuses the reload, carrying
/// `hyprctl`'s own complaint when it had one.
pub fn apply_display_layout(cx: &SessionCx<'_>, layout: &DisplayLayout) -> Result<(), ApplyError> {
    if layout.displays.is_empty() || !layout.displays.iter().any(DisplayEntry::enabled) {
        return Err(ApplyError::Layout(
            "At least one display must remain enabled".to_owned(),
        ));
    }
    // Strict here, lenient in the renderer: this is the path that has to reject a mirror
    // Hyprland would log and ignore, because a layout that quietly stopped matching the file
    // it was saved from is the failure the whole confirm-or-revert dance exists to catch.
    let mirrors = strict_mirror_targets(&layout.displays)?;
    let placed: Vec<&DisplayEntry> = layout
        .displays
        .iter()
        .filter(|entry| entry.enabled() && !mirrors.iter().any(|(name, _)| *name == entry.output()))
        .collect();
    if placed.is_empty() {
        return Err(ApplyError::Layout(
            "At least one display must show the desktop rather than mirror another".to_owned(),
        ));
    }
    let rectangles = rectangles_of(&placed)?;
    let adjacency = check_geometry(&rectangles)?;
    check_connected(&adjacency)?;
    render_displays(cx.render(), layout)?;
    let result = run(cx, &["hyprctl", "reload"]);
    if result.status != 0 {
        let detail = result.stderr.trim();
        return Err(ApplyError::Layout(if detail.is_empty() {
            "Unable to reload the display layout".to_owned()
        } else {
            detail.to_owned()
        }));
    }
    Ok(())
}

/// Each placed display's rectangle, its size divided by its own scale.
///
/// `float(first.get("output"))`'s neighbours default the way the Python's do: width and
/// height to 1 and scale to 1, so a record carrying no size at all is a one-pixel display
/// rather than a division by zero.
fn rectangles_of(placed: &[&DisplayEntry]) -> Result<Vec<Rectangle>, ApplyError> {
    let mut rectangles = Vec::with_capacity(placed.len());
    for entry in placed {
        let scale = number(entry, "scale", 1.0)?;
        let width = number(entry, "width", 1.0)? / scale;
        let height = number(entry, "height", 1.0)? / scale;
        let left = number(entry, "x", 0.0)?;
        let top = number(entry, "y", 0.0)?;
        rectangles.push(Rectangle {
            // `f'{first.get("output")}'` -- the raw value, not `str(...)` of a defaulted one,
            // so a record with no output at all names itself `None` in the overlap message.
            output: entry.get("output").map_or_else(
                || "None".to_owned(),
                garage_render::displays::LayoutValue::py_str,
            ),
            left,
            top,
            right: left + width,
            bottom: top + height,
        });
    }
    Ok(rectangles)
}

fn number(entry: &DisplayEntry, key: &str, fallback: f64) -> Result<f64, ApplyError> {
    Ok(entry
        .get(key)
        .map_or(Ok(fallback), garage_render::displays::LayoutValue::py_float)?)
}

/// Every pair checked once: an overlap refuses the layout, a touch records an edge.
///
/// Returns the adjacency list, one entry per rectangle, holding the indices it touches.
fn check_geometry(rectangles: &[Rectangle]) -> Result<Vec<Vec<usize>>, ApplyError> {
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); rectangles.len()];
    for (index, first) in rectangles.iter().enumerate() {
        for (offset, second) in rectangles.iter().enumerate().skip(index + 1) {
            let overlaps = first.left < second.right - TOLERANCE
                && first.right > second.left + TOLERANCE
                && first.top < second.bottom - TOLERANCE
                && first.bottom > second.top + TOLERANCE;
            if overlaps {
                return Err(ApplyError::Layout(format!(
                    "{} overlaps {}; drag their edges apart before applying",
                    first.output, second.output
                )));
            }
            let vertical = first.bottom.min(second.bottom) > first.top.max(second.top);
            let horizontal = first.right.min(second.right) > first.left.max(second.left);
            let side_by_side = ((first.right - second.left).abs() < TOLERANCE
                || (second.right - first.left).abs() < TOLERANCE)
                && vertical;
            let stacked = ((first.bottom - second.top).abs() < TOLERANCE
                || (second.bottom - first.top).abs() < TOLERANCE)
                && horizontal;
            if side_by_side || stacked {
                link(&mut adjacency, index, offset);
            }
        }
    }
    Ok(adjacency)
}

/// Record one touching pair, in both directions.
fn link(adjacency: &mut [Vec<usize>], first: usize, second: usize) {
    if let Some(ours) = adjacency.get_mut(first) {
        ours.push(second);
    }
    if let Some(theirs) = adjacency.get_mut(second) {
        theirs.push(first);
    }
}

/// Every display reachable from the first by crossing touching edges, or the gap refusal.
fn check_connected(adjacency: &[Vec<usize>]) -> Result<(), ApplyError> {
    let mut connected = vec![false; adjacency.len()];
    let mut pending = vec![0usize];
    if let Some(first) = connected.first_mut() {
        *first = true;
    }
    let mut reached = 1;
    while let Some(current) = pending.pop() {
        // `adjacency[current] - connected` in the Python: the neighbours not already seen.
        let fresh: Vec<usize> = adjacency
            .get(current)
            .into_iter()
            .flatten()
            .copied()
            .filter(|at| connected.get(*at) == Some(&false))
            .collect();
        for neighbour in fresh {
            if let Some(seen) = connected.get_mut(neighbour) {
                *seen = true;
            }
            reached += 1;
            pending.push(neighbour);
        }
    }
    if reached != adjacency.len() {
        return Err(ApplyError::Layout(
            "Display layout contains gaps; connect every monitor edge-to-edge".to_owned(),
        ));
    }
    Ok(())
}
