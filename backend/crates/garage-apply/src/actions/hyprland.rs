//! Shared live Hyprland geometry for pointer-driven actions.
//!
//! `panel.toggle` and `menu.toggle` both target the output under the pointer. Keeping the
//! scale-aware, half-open hit test here means a shared monitor edge has one answer everywhere.

use serde_json::{Map, Value};

use crate::command::{json_list, json_object};
use crate::cx::SessionCx;
use crate::error::ApplyError;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OutputTarget {
    pub(super) name: String,
    pub(super) logical_width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Point {
    pub(super) x: f64,
    pub(super) y: f64,
}

pub(super) fn cursor_position(cx: &SessionCx<'_>) -> Option<Point> {
    point(&json_object(cx, &["hyprctl", "cursorpos", "-j"]))
}

pub(super) fn output_at_cursor(cx: &SessionCx<'_>) -> Result<OutputTarget, ApplyError> {
    let point = cursor_position(cx).ok_or_else(no_monitor)?;
    json_list(cx, &["hyprctl", "monitors", "-j"])
        .iter()
        .filter_map(monitor_geometry)
        .find(|(_, x, y, width, height)| {
            point.x >= *x && point.x < *x + *width && point.y >= *y && point.y < *y + *height
        })
        .map(|(name, _, _, logical_width, _)| OutputTarget {
            name,
            logical_width,
        })
        .ok_or_else(no_monitor)
}

fn no_monitor() -> ApplyError {
    ApplyError::Settings("No monitor under cursor".to_owned())
}

fn point(cursor: &Map<String, Value>) -> Option<Point> {
    Some(Point {
        x: cursor.get("x")?.as_f64()?,
        y: cursor.get("y")?.as_f64()?,
    })
}

fn monitor_geometry(value: &Value) -> Option<(String, f64, f64, f64, f64)> {
    let monitor = value.as_object()?;
    let scale = monitor.get("scale")?.as_f64()?;
    if scale <= 0.0 {
        return None;
    }
    Some((
        monitor.get("name")?.as_str()?.to_owned(),
        monitor.get("x")?.as_f64()?,
        monitor.get("y")?.as_f64()?,
        monitor.get("width")?.as_f64()? / scale,
        monitor.get("height")?.as_f64()? / scale,
    ))
}
