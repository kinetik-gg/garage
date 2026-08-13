//! What every module ends its run by printing: one line of Waybar's `custom` module
//! JSON, `{"text": ..., "tooltip": ..., "class": ...}`.
//!
//! Both Python scripts build this by hand with `json.dumps(...)` on a plain dict and
//! its default arguments: `ensure_ascii=True` (every non-ASCII character, and the
//! ASCII DEL byte, comes out as a `\uXXXX` escape rather than raw UTF-8) and the
//! default separators `", "` / `": "`. [`json_string`] reproduces that encoder
//! byte-for-byte instead of leaning on `serde_json`'s formatter, which does not escape
//! non-ASCII by default and would silently stop matching the Python the moment a
//! title, an artist name or the microphone glyph carried one.

use std::io::Write as _;

/// The three keys every emitted line carries, in the order the Python dicts were
/// built in -- `payload()` in `media-status.py` and the dict literal in
/// `context-status.py`'s `emit()`. `json.dumps` preserves dict insertion order, so this
/// struct's field order **is** the wire order; do not reorder it without checking both
/// Pythons agree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Payload {
    pub(crate) text: String,
    pub(crate) tooltip: String,
    pub(crate) class: String,
}

impl Payload {
    /// `payload()`/`emit()` called with no arguments: nothing to show, idle styling.
    /// Every failure path in both scripts -- caught exception, empty selection, no
    /// argument at all -- ends here.
    #[must_use]
    pub(crate) fn idle() -> Self {
        Self {
            text: String::new(),
            tooltip: String::new(),
            class: "idle".to_string(),
        }
    }

    /// Render this payload as the single JSON line Waybar reads, matching
    /// `json.dumps({"text": ..., "tooltip": ..., "class": ...})` exactly.
    #[must_use]
    pub(crate) fn to_json_line(&self) -> String {
        format!(
            "{{\"text\": {}, \"tooltip\": {}, \"class\": {}}}",
            json_string(&self.text),
            json_string(&self.tooltip),
            json_string(&self.class),
        )
    }

    /// `print(json.dumps(...), flush=True)` (media) / `print(json.dumps(...))`
    /// (context): one line to stdout. Both Pythons end up flushed by the time the
    /// interpreter exits either way, so the two calls need no different treatment
    /// here; a write failure (a closed pipe) is swallowed exactly as an unflushed,
    /// about-to-exit Python process would lose it too.
    pub(crate) fn emit(&self) {
        let mut stdout = std::io::stdout();
        let _ = writeln!(stdout, "{}", self.to_json_line());
        let _ = stdout.flush();
    }
}

/// Encode `value` as a double-quoted JSON string using `CPython`'s `json.dumps`
/// `ensure_ascii=True` escaping table: `"` and `\` get their two-character escapes,
/// the control characters with short forms (`\b \f \n \r \t`) use them, every other
/// byte below `0x20` and everything above `0x7e` (ASCII DEL included) becomes
/// `\u00hh`, and a character outside the Basic Multilingual Plane is written as a
/// UTF-16 surrogate pair, lowercase hex throughout. Characters in `0x20..=0x7e` other
/// than the two escaped ones pass through unchanged.
#[must_use]
pub(crate) fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        push_escaped(&mut out, ch);
    }
    out.push('"');
    out
}

fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\u{8}' => out.push_str("\\b"),
        '\u{c}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        c if (c as u32) < 0x20 || (c as u32) > 0x7e => push_unicode_escape(out, c),
        c => out.push(c),
    }
}

fn push_unicode_escape(out: &mut String, ch: char) {
    use std::fmt::Write as _;

    let code_point = ch as u32;
    if code_point < 0x1_0000 {
        let _ = write!(out, "\\u{code_point:04x}");
        return;
    }
    let offset = code_point - 0x1_0000;
    let high = 0xd800 + (offset >> 10);
    let low = 0xdc00 + (offset & 0x3ff);
    let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
}

#[cfg(test)]
mod tests {
    use super::{json_string, Payload};

    #[test]
    fn plain_ascii_passes_through_unescaped() {
        assert_eq!("\"hello world\"", json_string("hello world"));
    }

    #[test]
    fn quote_and_backslash_get_two_character_escapes() {
        assert_eq!("\"a\\\"b\\\\c\"", json_string("a\"b\\c"));
    }

    #[test]
    fn control_characters_use_the_short_forms() {
        assert_eq!("\"\\n\\r\\t\\b\\f\"", json_string("\n\r\t\u{8}\u{c}"));
    }

    #[test]
    fn other_control_bytes_use_u00hh() {
        assert_eq!("\"\\u0001\\u001f\"", json_string("\u{1}\u{1f}"));
    }

    #[test]
    fn non_ascii_bmp_characters_are_escaped_lowercase() {
        // The microphone glyph's BLACK CIRCLE, U+25CF, and the em dash the media
        // module joins "artist" and "title" with -- Python's default
        // ensure_ascii=True turns each into a lowercase \uXXXX escape, never the
        // raw UTF-8 byte.
        assert_eq!("\"\\u25cf\"", json_string("\u{25cf}"));
        assert_eq!("\"\\u2014\"", json_string("\u{2014}"));
    }

    #[test]
    fn astral_characters_become_surrogate_pairs() {
        // U+1F3B5 MUSICAL NOTE: Python's json.dumps("\U0001F3B5") is
        // '"\\ud83c\\udfb5"'.
        assert_eq!("\"\\ud83c\\udfb5\"", json_string("\u{1F3B5}"));
    }

    #[test]
    fn del_is_escaped_even_though_it_is_ascii() {
        assert_eq!("\"\\u007f\"", json_string("\u{7f}"));
    }

    #[test]
    fn tilde_is_the_last_unescaped_byte() {
        assert_eq!("\"~\"", json_string("~"));
    }

    #[test]
    fn idle_payload_matches_pythons_bare_payload_call() {
        let idle = Payload::idle();
        assert_eq!("", idle.text);
        assert_eq!("", idle.tooltip);
        assert_eq!("idle", idle.class);
    }

    #[test]
    fn payload_json_shape_matches_json_dumps_field_order_and_separators() {
        let payload = Payload {
            text: "CTR 2".to_string(),
            tooltip: "a\nb".to_string(),
            class: "active".to_string(),
        };
        assert_eq!(
            "{\"text\": \"CTR 2\", \"tooltip\": \"a\\nb\", \"class\": \"active\"}",
            payload.to_json_line()
        );
    }
}
