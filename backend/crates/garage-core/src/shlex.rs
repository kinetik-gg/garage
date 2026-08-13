//! `shlex.quote()`: a shell-safe spelling of a string, byte-identical to `CPython`'s `shlex`
//! module.
//!
//! Two callers, both of them a shell fragment written for a human or a later shell to read
//! back: `render_region()`'s `export LANG=...` line in `locale.env` (garage:4541), and a
//! `note()` log line that echoes a command about to run (garage:6509). Neither is reachable
//! from an untrusted value -- a locale comes from [`crate::schema::coerce::Locale`], already
//! constrained -- but the quoting still has to be *exactly* `shlex.quote()`'s, because
//! `locale.env` is `source`d by a real shell on the next login and a quoting rule that
//! disagreed with Python's, even narrowly, would be a shell-syntax bug reachable only on
//! whatever locale string first exercised the gap.

/// `shlex.quote(s)`: `s` unchanged if every character is already shell-safe, otherwise `s`
/// single-quoted with each embedded `'` broken out as `'"'"'` -- close the quote, a
/// double-quoted single quote, reopen the quote. An empty string is `''`, not itself, since
/// the empty string is the one "safe" value a shell would otherwise see as no argument at all.
///
/// `CPython`'s own safe set (`_find_unsafe = re.compile(r'[^\w@%+=:,./-]', re.ASCII).search`):
/// ASCII letters, digits, underscore, and the eight punctuation characters `@%+=:,./-`. `\w`
/// is `[A-Za-z0-9_]` under `re.ASCII`, which is the flag the Python passes.
#[must_use]
pub fn shlex_quote(text: &str) -> String {
    if text.is_empty() {
        return "''".to_owned();
    }
    if text.bytes().all(is_shell_safe) {
        return text.to_owned();
    }
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('\'');
    for ch in text.chars() {
        if ch == '\'' {
            quoted.push_str("'\"'\"'");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

/// One byte of `_find_unsafe`'s complement: `[A-Za-z0-9_@%+=:,./-]`.
fn is_shell_safe(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'_' | b'@' | b'%' | b'+' | b'=' | b':' | b',' | b'.' | b'/' | b'-'
        )
}

#[cfg(test)]
mod tests {
    use super::shlex_quote;

    #[test]
    fn safe_strings_pass_through_unchanged() {
        assert_eq!(shlex_quote("en_US.UTF-8"), "en_US.UTF-8");
        assert_eq!(shlex_quote("hello"), "hello");
        assert_eq!(shlex_quote("a/b:c=d,e.f-g@h%i+j"), "a/b:c=d,e.f-g@h%i+j");
    }

    #[test]
    fn the_empty_string_quotes_to_two_single_quotes() {
        assert_eq!(shlex_quote(""), "''");
    }

    #[test]
    fn unsafe_strings_are_single_quoted() {
        assert_eq!(shlex_quote("hello world"), "'hello world'");
        assert_eq!(shlex_quote("a;b"), "'a;b'");
        assert_eq!(shlex_quote("$HOME"), "'$HOME'");
    }

    #[test]
    fn an_embedded_single_quote_is_broken_out_and_reopened() {
        assert_eq!(shlex_quote("it's"), "'it'\"'\"'s'");
        assert_eq!(shlex_quote("'"), "''\"'\"''");
    }
}
