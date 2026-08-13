//! The two wire formats a display layout travels in, and the conversions between them and
//! [`LayoutValue`].
//!
//! The Displays pane sends a candidate layout as the JSON argument to `display-test`, and the
//! pending-transaction file is JSON as well -- `json.dumps(pending)`, holding the candidate,
//! the layout it replaced and the token that ties the two ends of the transaction together.
//! `hyprctl monitors all -j` is JSON too, which is where `display_snapshot()` gets its
//! records. So JSON is this crate's boundary format, while the *value* every caller works on
//! is [`LayoutValue`] -- which `garage-render` owns, because `render_displays()` consumes it
//! and cannot name `serde_json`.
//!
//! [`to_json`] is written out by hand rather than through a `Serialize` impl for one reason:
//! `json.dumps()`'s defaults are the contract for the pending file's bytes, and they are not
//! `serde_json::to_string`'s. `CPython` separates members with `", "` and keys from values
//! with `": "`, and escapes every non-ASCII character as `\uXXXX` (`ensure_ascii=True`).
//! `serde_json` does neither.

use garage_core::pyrepr::py_float_repr;
use garage_render::displays::{DisplayEntry, DisplayLayout, LayoutValue};
use serde_json::Value;

/// One JSON value as a layout value.
///
/// A number that has no fraction and fits an `i64` becomes [`LayoutValue::Int`], which is the
/// split `json.loads()` makes: `1` parses to a Python `int` and `1.0` to a `float`, and the
/// difference reaches the emitted `displays.toml` as `x = 1` versus `x = 1.0`.
pub(crate) fn from_json(value: &Value) -> LayoutValue {
    match value {
        Value::Null => LayoutValue::Null,
        Value::Bool(flag) => LayoutValue::Bool(*flag),
        Value::Number(number) => number.as_i64().map_or_else(
            || LayoutValue::Float(number.as_f64().unwrap_or_default()),
            LayoutValue::Int,
        ),
        Value::String(text) => LayoutValue::Str(text.clone()),
        Value::Array(items) => LayoutValue::Array(items.iter().map(from_json).collect()),
        Value::Object(fields) => LayoutValue::Table(
            fields
                .iter()
                .map(|(key, held)| (key.clone(), from_json(held)))
                .collect(),
        ),
    }
}

/// One display record out of a JSON object. Anything that is not an object is an empty
/// record, which answers every `.get()` with its default -- the nearest survivable reading of
/// a pane that sent the wrong shape.
pub(crate) fn entry_from_json(value: &Value) -> DisplayEntry {
    match value {
        Value::Object(fields) => DisplayEntry::from_fields(
            fields
                .iter()
                .map(|(key, held)| (key.clone(), from_json(held)))
                .collect(),
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            DisplayEntry::default()
        }
    }
}

/// A whole layout out of the `display-test` payload: `{"primary": str, "displays": [...]}`.
///
/// `layout.get("primary", "")` and `layout.get("displays", [])`, with the same leniency the
/// Python has by accident: a payload whose `displays` is not a list iterates into nothing
/// here, where the Python would raise `TypeError`. That difference reaches the user as the
/// "At least one display must remain enabled" refusal instead of a traceback.
pub(crate) fn layout_from_json(value: &Value) -> DisplayLayout {
    DisplayLayout {
        primary: value
            .get("primary")
            .map_or_else(String::new, |held| from_json(held).py_str()),
        displays: value
            .get("displays")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(entry_from_json).collect())
            .unwrap_or_default(),
    }
}

/// `json.dumps(value)` for a layout value: `CPython`'s defaults, which are not
/// `serde_json`'s. See the module doc.
pub(crate) fn to_json(value: &LayoutValue, out: &mut String) {
    match value {
        LayoutValue::Null => out.push_str("null"),
        LayoutValue::Bool(true) => out.push_str("true"),
        LayoutValue::Bool(false) => out.push_str("false"),
        LayoutValue::Int(number) => out.push_str(&number.to_string()),
        // `json.dumps` spells a float with `float.__repr__`, which is `py_float_repr`.
        LayoutValue::Float(number) => out.push_str(&py_float_repr(*number)),
        LayoutValue::Str(text) | LayoutValue::Datetime(text) => write_string(text, out),
        LayoutValue::Array(items) => {
            out.push('[');
            for (at, item) in items.iter().enumerate() {
                if at > 0 {
                    out.push_str(", ");
                }
                to_json(item, out);
            }
            out.push(']');
        }
        LayoutValue::Table(entries) => {
            out.push('{');
            for (at, (key, held)) in entries.iter().enumerate() {
                if at > 0 {
                    out.push_str(", ");
                }
                write_string(key, out);
                out.push_str(": ");
                to_json(held, out);
            }
            out.push('}');
        }
    }
}

/// `json.dumps`'s string encoder with `ensure_ascii=True`: the six named escapes, `\u00xx`
/// for the remaining C0 controls, and `\uXXXX` -- surrogate pairs above the BMP -- for
/// everything outside ASCII.
pub(crate) fn write_string(text: &str, out: &mut String) {
    use std::fmt::Write as _;

    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            _ if ch < '\u{20}' => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            _ if ch.is_ascii() => out.push(ch),
            _ => {
                let mut units = [0u16; 2];
                for unit in ch.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// One display record back to JSON, for the snapshot envelope and the pending file.
pub(crate) fn entry_to_json(entry: &DisplayEntry, out: &mut String) {
    let fields: Vec<(String, LayoutValue)> = entry.fields().to_vec();
    to_json(&LayoutValue::Table(fields), out);
}
