//! Quoting a string as a Lua string literal -- `lua_string()`.
//!
//! Not `serde_json`: JSON and Lua only look alike. JSON's default `\uXXXX` escape for
//! non-ASCII is a syntax error in Lua, which spells the same thing `\u{XXXX}`, and Lua
//! rejects a raw newline inside a quoted literal where JSON never has one to reject.
//! Non-ASCII is emitted as UTF-8 rather than escaped: the fragment is written UTF-8 and Lua
//! takes string literals as bytes, so the two are the same string and the generated file
//! stays readable.
//!
//! The escape table itself is short on purpose: the two characters that end a Lua `"..."`
//! literal early (`\` and `"`), plus the whitespace Lua's lexer treats as a line break
//! inside one (`\n`, `\r`, `\t`). Everything else printable passes through verbatim, and
//! anything below `0x20` or `0x7f` is escaped braced -- `\u{XXXX}` rather than Lua's decimal
//! `\ddd` -- so the escape cannot run into a literal digit that follows it in the source.
//!
//! Doc-only: this crate's stub convention gives a module one `pub(crate)` entry point per
//! renderer that writes a file and reports `Result<(), RenderError>`. `lua_string()` and its
//! neighbours return `String`, not a write outcome, so they stay documented rather than
//! stubbed with a signature Phase 3 would only have to undo.

use std::fmt::Write as _;

/// `LUA_ESCAPES` (garage:2054-2060): the only characters that get a short escape rather than
/// passing through or falling into the braced `\u{XXXX}` form below.
const fn short_escape(ch: char) -> Option<&'static str> {
    match ch {
        '\\' => Some("\\\\"),
        '"' => Some("\\\""),
        '\n' => Some("\\n"),
        '\r' => Some("\\r"),
        '\t' => Some("\\t"),
        _ => None,
    }
}

/// Quote a string as a Lua string literal (`lua_string()`, garage:2063-2079).
///
/// See the module docs for why this is not `serde_json`, and why non-ASCII passes through
/// as UTF-8 rather than being escaped.
#[must_use]
pub(crate) fn lua_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if let Some(escape) = short_escape(ch) {
            out.push_str(escape);
        } else if ch < ' ' || ch == '\u{7f}' {
            // Braced so the escape cannot run into a literal digit that follows it in the
            // source, the way Lua's own decimal `\ddd` escape can.
            let _ = write!(out, "\\u{{{:x}}}", ch as u32);
        } else {
            out.push(ch);
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::lua_string;

    #[test]
    fn plain_text_passes_through_quoted() {
        assert_eq!(lua_string("us"), "\"us\"");
        assert_eq!(lua_string(""), "\"\"");
    }

    #[test]
    fn the_five_short_escapes_are_used() {
        assert_eq!(lua_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(lua_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(lua_string("a\nb"), "\"a\\nb\"");
        assert_eq!(lua_string("a\rb"), "\"a\\rb\"");
        assert_eq!(lua_string("a\tb"), "\"a\\tb\"");
    }

    #[test]
    fn control_characters_below_0x20_are_braced_hex() {
        assert_eq!(lua_string("\u{1}"), "\"\\u{1}\"");
        assert_eq!(lua_string("\u{0}"), "\"\\u{0}\"");
        assert_eq!(lua_string("a\u{b}b"), "\"a\\u{b}b\"");
    }

    #[test]
    fn del_is_braced_hex_like_the_c0_controls() {
        assert_eq!(lua_string("\u{7f}"), "\"\\u{7f}\"");
    }

    #[test]
    fn a_brace_escape_cannot_run_into_a_following_digit() {
        // Lua's own \ddd form would swallow the "2" that follows; the braced
        // spelling cannot, because `}` ends it unambiguously.
        assert_eq!(lua_string("\u{1}2"), "\"\\u{1}2\"");
    }

    #[test]
    fn non_ascii_passes_through_as_utf8() {
        assert_eq!(lua_string("café"), "\"café\"");
        assert_eq!(lua_string("日本語"), "\"日本語\"");
    }
}
