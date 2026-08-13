//! A hand-rolled `Json` value and writer, and [`response`] -- the one line of compact JSON
//! every non-`run` command prints.
//!
//! No `serde_json`: nothing else in this workspace depends on it, the payloads are four
//! small, fixed shapes (an envelope, a status record, a refresh record, a list of search
//! hits), and the byte-parity requirement is narrow enough -- match `CPython`'s
//! `json.dumps(value, ensure_ascii=False, separators=(",", ":"))` for `bool`, `int`, `str`,
//! `list` and `dict` -- that a writer earns its keep in under a hundred lines. Pulling in a
//! general serializer (and a `Serialize` derive on shapes that exist only to be printed once)
//! would be more surface for the same guarantee.
//!
//! [`Json::Object`] is a `Vec<(String, Json)>` rather than a map: `json.dumps` on a `dict`
//! preserves insertion order, which every caller here relies on to reproduce the Python's
//! literal key order (`{"ok": ..., "data": ..., "error": ...}`, not alphabetical), and a
//! `Vec` makes that the only order there is to preserve.

use std::fmt::Write as _;

/// A JSON value, restricted to what this binary's four payloads ever hold: no floats, no
/// `u64` past `i64::MAX` -- `CPython`'s own `int` is what every count, timestamp and
/// duration here started as, and all of them fit.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<Json>),
    /// Ordered key/value pairs, printed in insertion order.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// A string field, from anything that converts to one -- the common case at every call
    /// site, which otherwise reads `Json::Str(value.to_string())` at every field.
    pub(crate) fn str(value: impl Into<String>) -> Self {
        Self::Str(value.into())
    }

    /// Render this value the way `json.dumps(value, ensure_ascii=False,
    /// separators=(",", ":"))` would.
    #[must_use]
    pub(crate) fn dump(&self) -> String {
        let mut out = String::new();
        write_value(self, &mut out);
        out
    }
}

fn write_value(value: &Json, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Int(number) => {
            let _ = write!(out, "{number}");
        }
        Json::Str(text) => write_string(text, out),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Json::Object(fields) => {
            out.push('{');
            for (index, (key, item)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write_value(item, out);
            }
            out.push('}');
        }
    }
}

/// A JSON string, escaped the way `CPython`'s `encode_basestring` (the `ensure_ascii=False`
/// encoder, `json/encoder.py`) escapes it: `\`, `"` and the C0 controls U+0000..U+001F get
/// their `ESCAPE_DCT` spelling -- the six short escapes for backslash, quote, backspace,
/// form feed, newline, carriage return and tab, `\u00xx` for the rest of that range -- and
/// every other code point, ASCII or not, is written out verbatim as UTF-8. That last part is
/// exactly what `ensure_ascii=False` means: nothing above U+001F is ever escaped, not even
/// DEL or the non-ASCII paths a `~/Documents` in another script can hold.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0}'..='\u{1f}' => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
}

/// Print the one-line envelope every non-`run` command answers with:
/// `{"ok":<not error>,"data":<data>,"error":<error>}`, exactly as the Python's `response()`
/// does with `print(json.dumps(...))` -- one line, LF-terminated by `print`, to stdout.
pub(crate) fn response(data: Option<Json>, error: &str) {
    let envelope = Json::Object(vec![
        ("ok".to_string(), Json::Bool(error.is_empty())),
        ("data".to_string(), data.unwrap_or(Json::Null)),
        ("error".to_string(), Json::str(error)),
    ]);
    println!("{}", envelope.dump());
}

#[cfg(test)]
mod tests {
    use super::{response, Json};

    #[test]
    fn scalars_and_containers_match_python_compact_json() {
        assert_eq!(Json::Null.dump(), "null");
        assert_eq!(Json::Bool(true).dump(), "true");
        assert_eq!(Json::Int(-8).dump(), "-8");
        assert_eq!(Json::str("hi").dump(), "\"hi\"");
        assert_eq!(
            Json::Array(vec![Json::Int(1), Json::Int(2)]).dump(),
            "[1,2]"
        );
        assert_eq!(
            Json::Object(vec![
                ("b".to_string(), Json::Int(2)),
                ("a".to_string(), Json::Int(1)),
            ])
            .dump(),
            // Insertion order, not alphabetical.
            "{\"b\":2,\"a\":1}"
        );
    }

    #[test]
    fn strings_escape_like_ensure_ascii_false() {
        assert_eq!(Json::str("a\\b\"c").dump(), "\"a\\\\b\\\"c\"");
        assert_eq!(Json::str("\t\n\r").dump(), "\"\\t\\n\\r\"");
        assert_eq!(Json::str("\u{0}\u{1f}").dump(), "\"\\u0000\\u001f\"");
        // Non-ASCII is written verbatim -- ensure_ascii=False, not \uXXXX.
        assert_eq!(Json::str("café").dump(), "\"café\"");
        // DEL and other high control-ish points above 0x1f are not escaped either.
        assert_eq!(Json::str("\u{7f}").dump(), "\"\u{7f}\"");
    }

    #[test]
    fn envelope_shape_matches_the_python_response_function() {
        // Success: ok is true, error is "".
        let mut buf = Vec::new();
        {
            use std::io::Write as _;
            // response() prints directly; reproduce its exact string here instead of
            // capturing stdout, since capturing process stdout is out of reach for a unit
            // test and the string form is what matters.
            let envelope = Json::Object(vec![
                ("ok".to_string(), Json::Bool(true)),
                ("data".to_string(), Json::Null),
                ("error".to_string(), Json::str("")),
            ]);
            write!(buf, "{}", envelope.dump()).unwrap();
        }
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "{\"ok\":true,\"data\":null,\"error\":\"\"}"
        );
    }

    /// `response()` itself, exercised for its side effect rather than its return value: it
    /// must not panic on either branch.
    #[test]
    fn response_does_not_panic() {
        response(Some(Json::Int(1)), "");
        response(None, "boom");
    }
}
