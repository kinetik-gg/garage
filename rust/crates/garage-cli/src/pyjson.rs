//! `CPython`-compatible `json.dumps(value, separators=(",", ":"))`, for the envelope.
//!
//! `response()` is the only thing this binary prints for a machine to read, and the QML
//! client parses it, so the bytes are a contract. `serde_json::to_string` gets two of the
//! three rules right by accident and the third wrong: it writes the compact separators
//! (which is what `separators=(",", ":")` asks for) but passes non-ASCII through as UTF-8,
//! where `json.dumps` defaults to `ensure_ascii=True` and escapes every code point above
//! `~` as `\uXXXX`. A wallpaper path with an accent in it, a locale name, a `luac` complaint
//! carrying a smart quote -- all of them come out of the two backends differently unless the
//! escaping is done here.
//!
//! Deliberately a local copy rather than a dependency on `garage-ai-usage`'s
//! [`pyjson`](../../garage-ai-usage/src/pyjson.rs), which is the same encoder with the
//! *default* separators: that crate is a leaf binary and this one has no business depending
//! on it. The shared home for both is `garage-core`, as a follow-up -- there are two callers
//! now, which is the point at which a third would be one too many.

use std::fmt::Write as _;

use serde_json::{Map, Value};

/// One value, compact-separated and ASCII-escaped, appended to `out`.
pub(crate) fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        // `serde_json`'s `Number` prints an integer token as bare digits (Python `int`) and
        // a float token in plain decimal (Python `float` for every magnitude a preference
        // can hold -- every float in the schema is range-checked, so the `1e+20` form
        // Python's `repr` switches to is not reachable from here).
        Value::Number(number) => {
            let _ = write!(out, "{number}");
        }
        Value::String(text) => write_string(text, out),
        Value::Array(items) => write_array(items, out),
        Value::Object(map) => write_object(map, out),
    }
}

fn write_array(items: &[Value], out: &mut String) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_value(item, out);
    }
    out.push(']');
}

fn write_object(map: &Map<String, Value>, out: &mut String) {
    out.push('{');
    for (index, (key, item)) in map.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        write_string(key, out);
        out.push(':');
        write_value(item, out);
    }
    out.push('}');
}

/// `CPython`'s `py_encode_basestring_ascii()`: escape `"` and `\`, escape every control
/// character (the five with short forms -- `\b \t \n \f \r` -- and the rest as `\u00XX`),
/// and -- because `ensure_ascii=True` -- escape everything outside `0x20..=0x7e` as
/// `\uXXXX`, splitting code points above `0xffff` into a UTF-16 surrogate pair exactly as
/// the C encoder does.
pub(crate) fn write_string(text: &str, out: &mut String) {
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
    let code_point = u32::from(letter);
    if code_point > 0xffff {
        let shifted = code_point - 0x1_0000;
        let high = 0xd800 + (shifted >> 10);
        let low = 0xdc00 + (shifted & 0x3ff);
        let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
    } else {
        let _ = write!(out, "\\u{code_point:04x}");
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::write_value;

    fn dumps(value: &Value) -> String {
        let mut out = String::new();
        write_value(value, &mut out);
        out
    }

    /// Every expectation below is the literal `python3 -c 'import json; print(json.dumps(...,
    /// separators=(",", ":")))'` printed, pasted here rather than described.
    #[test]
    fn scalars_come_out_the_way_python_writes_them() {
        assert_eq!(dumps(&Value::Null), "null");
        assert_eq!(dumps(&json!(true)), "true");
        assert_eq!(dumps(&json!(false)), "false");
        assert_eq!(dumps(&json!(2)), "2");
        assert_eq!(dumps(&json!(1.0)), "1.0");
        assert_eq!(dumps(&json!(0.65)), "0.65");
        assert_eq!(dumps(&json!("dark")), "\"dark\"");
    }

    #[test]
    fn containers_carry_no_spaces_at_all() {
        assert_eq!(dumps(&json!([1, 2, 3])), "[1,2,3]");
        assert_eq!(dumps(&json!({"token": "1f2e3d"})), "{\"token\":\"1f2e3d\"}");
        assert_eq!(
            dumps(&json!({"active": true, "applied": false})),
            "{\"active\":true,\"applied\":false}"
        );
        assert_eq!(dumps(&json!([])), "[]");
        assert_eq!(dumps(&json!({})), "{}");
    }

    #[test]
    fn insertion_order_survives_because_the_map_preserves_it() {
        let nested = json!({"appearance": {"accent_color": "blue", "border_size": 2,
                                           "animation_speed": 1.0, "glass_refraction": 0.65,
                                           "reduce_motion": false}});
        assert_eq!(
            dumps(&nested),
            "{\"appearance\":{\"accent_color\":\"blue\",\"border_size\":2,\
             \"animation_speed\":1.0,\"glass_refraction\":0.65,\"reduce_motion\":false}}"
        );
    }

    #[test]
    fn every_non_ascii_code_point_is_escaped_the_way_ensure_ascii_escapes_it() {
        assert_eq!(
            dumps(&json!("\u{2728} caf\u{e9} \u{1f680} \t\"quo\\ted\" \u{1}")),
            "\"\\u2728 caf\\u00e9 \\ud83d\\ude80 \\t\\\"quo\\\\ted\\\" \\u0001\""
        );
    }

    #[test]
    fn del_is_outside_the_printable_range_python_draws() {
        assert_eq!(dumps(&json!("\u{7f}")), "\"\\u007f\"");
        assert_eq!(dumps(&json!("~")), "\"~\"");
        assert_eq!(dumps(&json!(" ")), "\" \"");
    }
}
