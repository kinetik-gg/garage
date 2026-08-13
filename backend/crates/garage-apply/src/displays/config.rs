//! `normalize_display_layout()` and `layout_toml()`: re-anchoring a layout at the origin, and
//! serialising it back into `displays.toml`.
//!
//! The reader half -- `load_display_config()`, `mirror_targets()` and the layout types
//! themselves -- is [`garage_render::displays`]'s, because `render_displays()` consumes it
//! and `garage-apply` depends on `garage-render` rather than the other way round. What is
//! left here is the half only an applier performs: normalising a candidate arrangement before
//! it is applied, and writing a confirmed one back out.
//!
//! `normalize_display_layout()` re-anchors the arrangement at `(0, 0)` from the outputs that
//! actually occupy the desktop. A mirror only carries a parked coordinate for when it is
//! turned off again, and anchoring on that would slide every real display away from the
//! origin -- so mirrors and disabled outputs are excluded from finding the corner, and then
//! shifted along with everything else once it is found.
//!
//! `layout_toml()` writes nine keys per record, each only when the record has it, plus
//! `mirror` when it is set to something. `mirror` is outside the loop deliberately:
//! extending is the default and every display carries the key, so the loop would leave a dead
//! `mirror = ""` in each entry -- but leaving it out of the loop entirely is what would
//! silently drop the mirror on the way to disk.
//!
//! Doc-only in the dispatch sense: both operate on a display-layout value rather than a
//! [`SessionCx`](crate::cx::SessionCx), and are reached from [`super::transaction`] and
//! [`super::apply`] rather than being dispatch targets themselves.

use std::fmt::Write as _;

use garage_core::toml_emit::{toml_value, Value};
use garage_render::displays::{mirror_targets, DisplayEntry, DisplayLayout, LayoutValue};

use crate::error::ApplyError;

/// The nine keys `layout_toml()` writes from the record, in the order it writes them.
/// `mirror` is not among them; see the module doc.
const WRITTEN_KEYS: [&str; 9] = [
    "output",
    "description",
    "enabled",
    "mode",
    "x",
    "y",
    "scale",
    "transform",
    "vrr",
];

/// `normalize_display_layout()` (garage:5158-5174): the same arrangement, with its top-left
/// corner at the origin.
///
/// # Errors
///
/// [`ApplyError::Number`] when a record puts something `float()` refuses where a coordinate
/// belongs.
pub fn normalize_display_layout(layout: &DisplayLayout) -> Result<DisplayLayout, ApplyError> {
    // `copy.deepcopy(layout)`: the caller's layout is never edited in place, because
    // `display_test()` keeps the *un*normalized payload nowhere and the pane's own copy is
    // the one it is still showing.
    let mut normalized = layout.clone();
    let mirrors = mirror_targets(&normalized.displays);
    let placed: Vec<&DisplayEntry> = normalized
        .displays
        .iter()
        .filter(|entry| entry.enabled() && !mirrors.iter().any(|(name, _)| *name == entry.output()))
        .collect();
    if placed.is_empty() {
        return Ok(normalized);
    }
    let mut minimum_x = f64::INFINITY;
    let mut minimum_y = f64::INFINITY;
    for entry in placed {
        minimum_x = minimum_x.min(coordinate(entry, "x")?);
        minimum_y = minimum_y.min(coordinate(entry, "y")?);
    }
    // Every display is shifted, not only the placed ones: a mirror's parked coordinate has to
    // move with the arrangement it will rejoin when the mirror is turned off.
    for entry in &mut normalized.displays {
        let x = py_round(coordinate(entry, "x")? - minimum_x);
        let y = py_round(coordinate(entry, "y")? - minimum_y);
        entry.set("x", LayoutValue::Int(x));
        entry.set("y", LayoutValue::Int(y));
    }
    Ok(normalized)
}

/// `float(item.get(key, 0))`.
fn coordinate(entry: &DisplayEntry, key: &str) -> Result<f64, ApplyError> {
    Ok(entry.get(key).map_or(Ok(0.0), LayoutValue::py_float)?)
}

/// `round(value)`: banker's rounding to an integer, which is Python's and not Rust's.
///
/// `round(0.5)` is `0` and `round(1.5)` is `2` -- halves go to the even neighbour, where
/// Rust's `f64::round` goes away from zero. Reachable: a pane that drags a display to a
/// half-pixel offset produces exactly such a tie.
fn py_round(value: f64) -> i64 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a display coordinate is a screen offset; the rounding is the operation and \
                  the magnitude is bounded by the desktop"
    )]
    let rounded = {
        // `round_ties_even` is the IEEE-754 roundTiesToEven that Python's `round()` performs.
        value.round_ties_even() as i64
    };
    rounded
}

/// `layout_toml()` (garage:5233-5248): the layout as `displays.toml`'s own text.
///
/// # Errors
///
/// [`ApplyError::Emit`] for a value `toml_value()` refuses -- an array or a table sitting
/// where a scalar belongs, or a non-finite float.
pub fn layout_toml(layout: &DisplayLayout) -> Result<String, ApplyError> {
    let mut text = format!(
        "primary = {}\n",
        toml_value(&Value::Str(layout.primary.clone()))?
    );
    for entry in &layout.displays {
        text.push_str("\n[[display]]\n");
        for key in WRITTEN_KEYS {
            if let Some(held) = entry.get(key) {
                let _ = writeln!(text, "{key} = {}", emitted(held)?);
            }
        }
        let mirror = entry.mirror();
        if !mirror.is_empty() {
            let _ = writeln!(text, "mirror = {}", toml_value(&Value::Str(mirror))?);
        }
    }
    Ok(text)
}

/// `toml_value(item[key])` for one layout value.
///
/// The four scalars go straight through the emitter. A container is handed across so the
/// emitter refuses it with the Python's own `Unsupported TOML value: {value!r}` message
/// rather than being silently dropped here; the two kinds the emitter has no case for at all
/// -- JSON's `null` and a hand-written TOML datetime -- are refused here with the same
/// message shape, since `toml_value(None)` in the Python raises exactly that.
fn emitted(value: &LayoutValue) -> Result<String, ApplyError> {
    let scalar = match value {
        LayoutValue::Bool(flag) => Value::Bool(*flag),
        LayoutValue::Int(number) => Value::Int(*number),
        LayoutValue::Float(number) => Value::Float(*number),
        LayoutValue::Str(text) => Value::Str(text.clone()),
        LayoutValue::Array(items) => Value::Array(items.iter().map(placeholder_for_repr).collect()),
        LayoutValue::Table(entries) => Value::Table(
            entries
                .iter()
                .map(|(key, held)| (key.clone(), placeholder_for_repr(held)))
                .collect(),
        ),
        LayoutValue::Null => {
            return Err(ApplyError::Layout(
                "Unsupported TOML value: None".to_owned(),
            ))
        }
        // A datetime's `repr()` is `datetime.datetime(2024, 1, 1, 0, 0)`, which would need a
        // calendar to reproduce from the source spelling; the source spelling is what is
        // reported instead. Reachable only from a hand-edited `displays.toml`, and only for a
        // key `layout_toml()` writes.
        LayoutValue::Datetime(stamp) => {
            return Err(ApplyError::Layout(format!(
                "Unsupported TOML value: {stamp}"
            )))
        }
    };
    Ok(toml_value(&scalar)?)
}

/// A container's members only reach the emitter's `repr()`, so a member the emitter has no
/// case for is spelled as the nearest thing it does have rather than failing separately.
fn placeholder_for_repr(value: &LayoutValue) -> Value {
    match value {
        LayoutValue::Bool(flag) => Value::Bool(*flag),
        LayoutValue::Int(number) => Value::Int(*number),
        LayoutValue::Float(number) => Value::Float(*number),
        LayoutValue::Str(text) | LayoutValue::Datetime(text) => Value::Str(text.clone()),
        LayoutValue::Null => Value::Str("None".to_owned()),
        LayoutValue::Array(items) => Value::Array(items.iter().map(placeholder_for_repr).collect()),
        LayoutValue::Table(entries) => Value::Table(
            entries
                .iter()
                .map(|(key, held)| (key.clone(), placeholder_for_repr(held)))
                .collect(),
        ),
    }
}
