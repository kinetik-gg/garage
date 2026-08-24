//! `CPython`-compatible JSON encoding for `serde_json::Value`.
//!
//! Python's default encoder preserves insertion order, writes spaces after separators,
//! and escapes every non-ASCII code point. `serde_json` preserves the first property in
//! this workspace but intentionally differs on the latter two. Both separator shapes the
//! retired Python tools used live here so wire formats do not each carry another copy of
//! the Unicode escaping table.

use std::fmt::Write as _;

use serde_json::Value;

/// Encode with `json.dumps(value)`'s default separators and `ensure_ascii=True`.
#[must_use]
pub fn dumps(value: &Value) -> String {
    encode(value, true)
}

/// Encode with `json.dumps(value, separators=(",", ":"))` and `ensure_ascii=True`.
#[must_use]
pub fn dumps_compact(value: &Value) -> String {
    encode(value, false)
}

fn encode(value: &Value, spaced: bool) -> String {
    let mut out = String::new();
    write_value(value, &mut out, spaced);
    out
}

fn write_value(value: &Value, out: &mut String, spaced: bool) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => {
            let _ = write!(out, "{number}");
        }
        Value::String(text) => write_string(text, out),
        Value::Array(items) => write_array(items, out, spaced),
        Value::Object(fields) => write_object(fields, out, spaced),
    }
}

fn write_array(items: &[Value], out: &mut String, spaced: bool) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
            if spaced {
                out.push(' ');
            }
        }
        write_value(item, out, spaced);
    }
    out.push(']');
}

fn write_object(fields: &serde_json::Map<String, Value>, out: &mut String, spaced: bool) {
    out.push('{');
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
            if spaced {
                out.push(' ');
            }
        }
        write_string(key, out);
        out.push(':');
        if spaced {
            out.push(' ');
        }
        write_value(value, out, spaced);
    }
    out.push('}');
}

/// Append one JSON string with `ensure_ascii=True` escaping.
pub fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for letter in text.chars() {
        match letter {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (' '..='~').contains(&other) => out.push(other),
            other => write_escaped(other, out),
        }
    }
    out.push('"');
}

fn write_escaped(letter: char, out: &mut String) {
    let point = u32::from(letter);
    if point > 0xffff {
        let shifted = point - 0x1_0000;
        let _ = write!(
            out,
            "\\u{:04x}\\u{:04x}",
            0xd800 + (shifted >> 10),
            0xdc00 + (shifted & 0x3ff)
        );
    } else {
        let _ = write!(out, "\\u{point:04x}");
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{dumps, dumps_compact};

    #[test]
    fn both_python_separator_shapes_are_available() {
        let value = json!({"text": "", "items": [1, 2]});
        assert_eq!(dumps(&value), r#"{"text": "", "items": [1, 2]}"#);
        assert_eq!(dumps_compact(&value), r#"{"text":"","items":[1,2]}"#);
    }

    #[test]
    fn non_ascii_and_control_text_is_ascii_escaped() {
        let value = json!("\u{2728} caf\u{e9} \u{1f680}\n\u{1}");
        assert_eq!(dumps(&value), r#""\u2728 caf\u00e9 \ud83d\ude80\n\u0001""#);
    }
}
