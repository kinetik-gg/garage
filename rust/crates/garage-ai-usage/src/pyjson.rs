//! `CPython`-compatible `json.dumps(value)` formatting, for the two lines this crate's
//! `main()` prints -- the only output this program has a byte-exact contract for.
//!
//! `main()` calls `json.dumps()` with no arguments in both places (`--bar` and `--json`),
//! which means the default separators -- `", "` between items, `": "` after a key -- and
//! `ensure_ascii=True`. `serde_json::to_string` gives neither: it writes compact `,`/`:`
//! with no space, and it passes non-ASCII text through as UTF-8 rather than escaping it.
//! `MenuBarPane.qml` and `AiUsagePalette.qml` parse whatever this prints, so the difference
//! is not cosmetic -- see the crate root docs for why the `SPARKLE` glyph in particular
//! depends on the `ensure_ascii` escaping done here.
//!
//! This is a writer, not a parser: [`crate::cache`] and [`crate::tokscale`] read JSON with
//! plain `serde_json` (whose formatting is never observed outside this process), and only
//! the two values handed to [`to_python_json`] -- the bar payload and the popover payload --
//! go through Python's rules on the way out.

use std::fmt::Write as _;

use serde_json::{Map, Value};

/// `json.dumps(value)`: default separators, `ensure_ascii=True`, insertion order preserved
/// (this crate's `serde_json::Map` is built with the `preserve_order` feature, matching
/// `CPython`'s dict order).
#[must_use]
pub(crate) fn to_python_json(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out);
    out
}

fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        // serde_json's `Number` prints an integer token as bare digits (matching Python
        // `int`) and a float token in plain decimal (matching Python `float` for every
        // magnitude this crate's data ever reaches). The one place the two diverge --
        // Python's `repr()` switching to `1e+20`-style scientific notation for very large
        // or very small floats -- is outside the range subscription percentages, costs and
        // token counts can produce; see the crate report.
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
            out.push_str(", ");
        }
        write_value(item, out);
    }
    out.push(']');
}

fn write_object(map: &Map<String, Value>, out: &mut String) {
    out.push('{');
    for (index, (key, item)) in map.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        write_string(key, out);
        out.push_str(": ");
        write_value(item, out);
    }
    out.push('}');
}

/// `CPython`'s `encoder.py_encode_basestring_ascii()`: escape `"` and `\`, escape every
/// control character (the five with short forms -- `\b \t \n \f \r` -- and the rest as
/// `\u00XX`), and -- because `ensure_ascii=True` -- escape everything outside `0x20..=0x7e`
/// as `\uXXXX`, splitting code points above `0xffff` into a UTF-16 surrogate pair the same
/// way the C encoder does. Everything printable-ASCII passes through unescaped.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if is_ascii_printable(other) => out.push(other),
            other => write_escaped(other, out),
        }
    }
    out.push('"');
}

/// `CPython`'s `ESCAPE_ASCII` boundary: the printable range is `0x20` (space) through
/// `0x7e` (`~`) inclusive. `0x7f` (DEL) is outside it and is escaped like any other
/// non-ASCII code point.
fn is_ascii_printable(ch: char) -> bool {
    (' '..='~').contains(&ch)
}

fn write_escaped(ch: char, out: &mut String) {
    let code_point = u32::from(ch);
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
    use super::to_python_json;
    use serde_json::{json, Value};

    #[test]
    fn separators_match_pythons_defaults() {
        let value = json!({"text": "", "tooltip": "", "class": "unavailable"});
        assert_eq!(
            to_python_json(&value),
            "{\"text\": \"\", \"tooltip\": \"\", \"class\": \"unavailable\"}"
        );
    }

    #[test]
    fn arrays_get_the_same_separator() {
        let value = json!([1, 2, 3]);
        assert_eq!(to_python_json(&value), "[1, 2, 3]");
    }

    #[test]
    fn key_order_is_preserved() {
        let mut map = serde_json::Map::new();
        map.insert("z".to_string(), Value::from(1));
        map.insert("a".to_string(), Value::from(2));
        let value = Value::Object(map);
        assert_eq!(to_python_json(&value), "{\"z\": 1, \"a\": 2}");
    }

    #[test]
    fn non_ascii_is_escaped_like_ensure_ascii() {
        // The Phosphor "sparkle" glyph, U+E6A2 -- the whole reason this module exists.
        let value = Value::String("\u{e6a2}".to_string());
        assert_eq!(to_python_json(&value), "\"\\ue6a2\"");
    }

    #[test]
    fn astral_code_points_become_a_surrogate_pair() {
        // A code point above U+FFFF, split into a UTF-16 surrogate pair the same way
        // CPython's C encoder does it.
        let value = Value::String("\u{1f600}".to_string());
        assert_eq!(to_python_json(&value), "\"\\ud83d\\ude00\"");
    }

    #[test]
    fn control_characters_use_short_escapes_where_python_has_them() {
        let value = Value::String("a\u{8}\u{c}\n\r\tb".to_string());
        assert_eq!(to_python_json(&value), "\"a\\b\\f\\n\\r\\tb\"");
    }

    #[test]
    fn other_control_characters_use_the_generic_escape() {
        let value = Value::String("a\u{1}b".to_string());
        assert_eq!(to_python_json(&value), "\"a\\u0001b\"");
    }

    #[test]
    fn quote_and_backslash_are_escaped() {
        let value = Value::String("say \"hi\" \\ bye".to_string());
        assert_eq!(to_python_json(&value), "\"say \\\"hi\\\" \\\\ bye\"");
    }

    #[test]
    fn del_is_escaped_though_it_is_ascii() {
        let value = Value::String("\u{7f}".to_string());
        assert_eq!(to_python_json(&value), "\"\\u007f\"");
    }

    #[test]
    fn null_and_bool_print_as_python_spells_them() {
        assert_eq!(to_python_json(&Value::Null), "null");
        assert_eq!(to_python_json(&Value::Bool(true)), "true");
        assert_eq!(to_python_json(&Value::Bool(false)), "false");
    }
}
