//! `json.loads(text)`, as far as these two files can exercise it.
//!
//! Only one caller reaches this -- `load_state`, reading back what the previous tick
//! wrote -- and it treats every failure the same way: `except (OSError, ValueError):
//! return {}`. So the errors here carry a message for a test to read and nothing the
//! production path branches on. What matters instead is that the *acceptances* match
//! `CPython`'s, because anything this rejects and Python accepts is a history silently
//! thrown away and a graph that goes flat.
//!
//! Two of those acceptances are easy to miss. `json.loads` takes the bare `NaN`,
//! `Infinity` and `-Infinity` tokens by default (`parse_constant`), which is how a
//! float that went non-finite survives a round trip through the state file. And a
//! number is an `int` exactly when it has neither a fraction nor an exponent, which is
//! the distinction the state file's two kinds of primed history turn on.

use super::{Object, Value};

/// How deep a document may nest before this gives up. `CPython`'s pure-Python decoder
/// recurses too and hits its own recursion limit at around a thousand frames; the C
/// accelerator raises `RecursionError`, which is not a `ValueError` and so would *not*
/// be caught by `load_state`. Refusing here instead keeps a hostile state file from
/// reaching the stack at all, and a state file this crate wrote nests three deep.
const MAX_DEPTH: usize = 100;

/// Why a document could not be read. Every variant is Python's `ValueError`, which is
/// what `json.JSONDecodeError` is a subclass of.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum ParseError {
    /// A token that is not valid JSON at all.
    #[error("expecting value at byte {0}")]
    Expecting(usize),
    /// Valid JSON followed by something else.
    #[error("extra data at byte {0}")]
    ExtraData(usize),
    /// Nesting past [`MAX_DEPTH`].
    #[error("too deeply nested")]
    TooDeep,
}

/// `json.loads(text)`.
///
/// # Errors
///
/// Returns [`ParseError`] for anything `json.loads` would raise a `ValueError` on.
pub(crate) fn loads(text: &str) -> Result<Value, ParseError> {
    let mut cursor = Cursor {
        bytes: text.as_bytes(),
        at: 0,
    };
    cursor.skip_whitespace();
    let value = cursor.value(0)?;
    cursor.skip_whitespace();
    if cursor.at < cursor.bytes.len() {
        return Err(ParseError::ExtraData(cursor.at));
    }
    Ok(value)
}

/// The input and how far into it we are. Byte-indexed rather than char-indexed: every
/// structural character in JSON is ASCII, and the only place a multi-byte sequence can
/// appear is inside a string, which is copied wholesale.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// The four characters `json`'s scanner treats as whitespace. Not Unicode
    /// whitespace: a non-breaking space between two tokens is an error in Python too.
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    /// Consume `word` if it is next, reporting whether it was.
    fn eat(&mut self, word: &str) -> bool {
        let end = self.at + word.len();
        if self.bytes.get(self.at..end) == Some(word.as_bytes()) {
            self.at = end;
            return true;
        }
        false
    }

    fn value(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth > MAX_DEPTH {
            return Err(ParseError::TooDeep);
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.list(depth),
            Some(b'"') => self.string().map(Value::Str),
            _ => self.atom(),
        }
    }

    /// The literals and the numbers: everything that is one token.
    fn atom(&mut self) -> Result<Value, ParseError> {
        let start = self.at;
        if self.eat("null") {
            return Ok(Value::Null);
        }
        if self.eat("true") {
            return Ok(Value::Bool(true));
        }
        if self.eat("false") {
            return Ok(Value::Bool(false));
        }
        if self.eat("NaN") {
            return Ok(Value::Float(f64::NAN));
        }
        if self.eat("Infinity") {
            return Ok(Value::Float(f64::INFINITY));
        }
        if self.eat("-Infinity") {
            return Ok(Value::Float(f64::NEG_INFINITY));
        }
        self.number().ok_or(ParseError::Expecting(start))
    }

    /// A number, and the decision of whether it is an `int` or a `float`.
    ///
    /// `CPython`'s scanner matches `-?(?:0|[1-9]\d*)(\.\d+)?([eE][-+]?\d+)?` and hands
    /// the fraction and exponent groups to `parse_float` when either is present and
    /// `parse_int` when neither is. This does the same by scanning the same shape and
    /// remembering whether it saw either. An integer too wide for an `i64` becomes a
    /// float rather than the arbitrary-precision `int` Python would build -- the one
    /// deliberate narrowing here, unreachable from a counter (`/proc` counters are 64
    /// bit) and noted rather than hidden.
    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        if self.integer_part() == 0 {
            self.at = start;
            return None;
        }
        let mut inexact = false;
        if self.peek() == Some(b'.') {
            self.at += 1;
            if self.digits() == 0 {
                self.at = start;
                return None;
            }
            inexact = true;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                self.at = start;
                return None;
            }
            inexact = true;
        }
        let text = std::str::from_utf8(self.bytes.get(start..self.at)?).ok()?;
        if inexact {
            return text.parse().ok().map(Value::Float);
        }
        Some(text.parse().map_or_else(
            |_| Value::Float(text.parse().unwrap_or(f64::NAN)),
            Value::Int,
        ))
    }

    /// The integer part, under JSON's `0 | [1-9]\d*` rule: a leading zero stands alone.
    /// `01` is not the number one -- `json.loads` reads the `0`, finds a `1` after it,
    /// and raises "Extra data" -- so consuming only the zero here is what puts the
    /// cursor where `loads` will notice.
    fn integer_part(&mut self) -> usize {
        if self.peek() == Some(b'0') {
            self.at += 1;
            return 1;
        }
        self.digits()
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        self.at - start
    }

    /// A string, with the six two-character escapes and `\uXXXX` including surrogate
    /// pairs. An unpaired surrogate becomes U+FFFD rather than an error, matching
    /// `CPython`, which builds a lone surrogate into the `str` it returns and only
    /// complains later if something tries to encode it.
    fn string(&mut self) -> Result<String, ParseError> {
        let opened = self.at;
        if self.peek() != Some(b'"') {
            return Err(ParseError::Expecting(opened));
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            match self.peek().ok_or(ParseError::Expecting(opened))? {
                b'"' => {
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.at += 1;
                    self.escape(&mut out, opened)?;
                }
                _ => self.raw_character(&mut out, opened)?,
            }
        }
    }

    /// One unescaped character, copied through as it stands. Decoded as UTF-8 rather
    /// than byte by byte so a multi-byte sequence stays one character.
    fn raw_character(&mut self, out: &mut String, opened: usize) -> Result<(), ParseError> {
        let rest = self
            .bytes
            .get(self.at..)
            .ok_or(ParseError::Expecting(opened))?;
        let text = std::str::from_utf8(rest).map_err(|_| ParseError::Expecting(opened))?;
        let ch = text.chars().next().ok_or(ParseError::Expecting(opened))?;
        out.push(ch);
        self.at += ch.len_utf8();
        Ok(())
    }

    fn escape(&mut self, out: &mut String, opened: usize) -> Result<(), ParseError> {
        let marker = self.peek().ok_or(ParseError::Expecting(opened))?;
        self.at += 1;
        let replacement = match marker {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => return self.unicode_escape(out, opened),
            _ => return Err(ParseError::Expecting(self.at - 1)),
        };
        out.push(replacement);
        Ok(())
    }

    fn unicode_escape(&mut self, out: &mut String, opened: usize) -> Result<(), ParseError> {
        let first = self.hex4().ok_or(ParseError::Expecting(opened))?;
        if !(0xd800..0xdc00).contains(&first) {
            out.push(char::from_u32(first).unwrap_or(char::REPLACEMENT_CHARACTER));
            return Ok(());
        }
        let mark = self.at;
        if let Some(combined) = self.low_surrogate().map(|second| pair(first, second)) {
            out.push(char::from_u32(combined).unwrap_or(char::REPLACEMENT_CHARACTER));
            return Ok(());
        }
        self.at = mark;
        out.push(char::REPLACEMENT_CHARACTER);
        Ok(())
    }

    /// The second half of a surrogate pair, if that is what comes next.
    fn low_surrogate(&mut self) -> Option<u32> {
        if !self.eat("\\u") {
            return None;
        }
        self.hex4()
            .filter(|second| (0xdc00..0xe000).contains(second))
    }

    fn hex4(&mut self) -> Option<u32> {
        let text = std::str::from_utf8(self.bytes.get(self.at..self.at + 4)?).ok()?;
        let value = u32::from_str_radix(text, 16).ok()?;
        self.at += 4;
        Some(value)
    }

    fn list(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.at += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Value::List(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::List(items));
                }
                _ => return Err(ParseError::Expecting(self.at)),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.at += 1;
        let mut fields = Object::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Value::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(ParseError::Expecting(self.at));
            }
            self.at += 1;
            self.skip_whitespace();
            // Python's dict assignment, so a repeated key keeps its first position and
            // takes the last value -- which is what `json.loads` leaves behind too.
            fields.insert(key, self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Object(fields));
                }
                _ => return Err(ParseError::Expecting(self.at)),
            }
        }
    }
}

/// The code point a high and a low surrogate stand for together.
fn pair(high: u32, low: u32) -> u32 {
    0x1_0000 + ((high - 0xd800) << 10) + (low - 0xdc00)
}

#[cfg(test)]
mod tests {
    use super::loads;
    use crate::json::{dumps, Value};

    fn object_keys(text: &str) -> Vec<String> {
        match loads(text) {
            Ok(Value::Object(fields)) => fields.pairs().map(|(key, _)| key.to_string()).collect(),
            _ => panic!("expected an object"),
        }
    }

    #[test]
    fn a_state_file_round_trips_to_the_same_bytes() {
        let text = concat!(
            r#"{"history": [0.0, 12.5, 100.0], "display": "42%", "extra": "", "#,
            r#""tooltip_parts": ["CPU 42.0%"], "active": true, "counters": [1, 2], "#,
            r#""last_sample": 1786624230.3203576, "period": 2.0, "device": null}"#
        );
        assert_eq!(dumps(&loads(text).expect("parses")), text);
    }

    #[test]
    fn key_order_survives_the_round_trip() {
        assert_eq!(
            object_keys(r#"{"zeta": 1, "alpha": 2, "mid": 3}"#),
            ["zeta", "alpha", "mid"]
        );
    }

    #[test]
    fn an_int_stays_an_int_and_a_float_stays_a_float() {
        assert_eq!(loads("0"), Ok(Value::Int(0)));
        assert_eq!(loads("0.0"), Ok(Value::Float(0.0)));
        assert_eq!(loads("1e2"), Ok(Value::Float(100.0)));
        assert_eq!(loads("-3"), Ok(Value::Int(-3)));
        // The two histories mark_unavailable and push_history leave behind.
        assert_eq!(dumps(&loads("[0, 0]").expect("parses")), "[0, 0]");
        assert_eq!(dumps(&loads("[0.0, 0.0]").expect("parses")), "[0.0, 0.0]");
    }

    #[test]
    fn the_non_finite_constants_python_emits_are_read_back() {
        assert!(matches!(loads("NaN"), Ok(Value::Float(number)) if number.is_nan()));
        assert_eq!(loads("Infinity"), Ok(Value::Float(f64::INFINITY)));
        assert_eq!(loads("-Infinity"), Ok(Value::Float(f64::NEG_INFINITY)));
    }

    #[test]
    fn escapes_decode_the_way_json_dumps_encoded_them() {
        assert_eq!(loads(r#""↓ x""#), Ok(Value::str("\u{2193} x")));
        assert_eq!(loads(r#""🎵""#), Ok(Value::str("\u{1F3B5}")));
        assert_eq!(
            loads(r#""\n\t\b\f\r\/\\\"""#),
            Ok(Value::str("\n\t\u{8}\u{c}\r/\\\""))
        );
    }

    #[test]
    fn a_bare_multibyte_character_is_one_character() {
        assert_eq!(loads("\"caf\u{e9}\""), Ok(Value::str("caf\u{e9}")));
    }

    #[test]
    fn whitespace_around_and_inside_a_document_is_ignored() {
        assert_eq!(
            loads("  \n{ \"a\" : [ 1 , 2 ] }\t "),
            loads(r#"{"a":[1,2]}"#)
        );
    }

    #[test]
    fn everything_load_state_has_to_reject_is_rejected() {
        for text in [
            "", "   ", "not json", "{", "[1,", "{\"a\"}", "{1: 2}", "1 2", "01", "1.", "-",
        ] {
            assert!(loads(text).is_err(), "{text:?} should not parse");
        }
    }

    #[test]
    fn nesting_past_the_cap_is_an_error_rather_than_a_blown_stack() {
        let deep = "[".repeat(500) + &"]".repeat(500);
        assert!(loads(&deep).is_err());
    }
}
