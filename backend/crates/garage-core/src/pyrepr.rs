//! `CPython`-compatible `repr()` and float formatting, for byte-identical output.
//!
//! Two callers need this, and both of them put the result somewhere a user or a
//! test can read:
//!
//!   * `toml_emit` writes floats into preferences.toml with `str(value)`, which
//!     for a Python float is `repr(value)`. A file the Python build wrote and a
//!     file the Rust build wrote have to compare equal byte for byte, or the
//!     rewrite-only-when-something-changed check in `compact_preferences_file`
//!     starts rewriting the file on every load.
//!   * `validate_preferences` formats the value it is replacing with `!r` --
//!     "`appearance.corner_radius` 'huge' is not valid, using 'normal'" -- and
//!     those notes go to stderr, which is the journal under the render units.
//!     A note that spells a value differently from the Python build is a note
//!     that no longer matches what the docs and the tests say it looks like.
//!
//! Nothing here reads a Unicode database. See [`py_str_repr`] for the one class
//! of input where that costs byte-parity, and why it does not reach these two
//! callers.

use std::fmt::Write as _;

/// A Python scalar, for the four types [`py_repr`] has to spell.
///
/// Borrowed rather than owned: every caller already holds the string it wants
/// formatted, and a repr is a thing you print rather than a thing you keep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PyValue<'a> {
    /// Python `bool`, which reprs as `True` / `False` rather than lowercase.
    Bool(bool),
    /// Python `int`, which reprs as its decimal digits with no suffix.
    Int(i64),
    /// Python `float`, which reprs through [`py_float_repr`].
    Float(f64),
    /// Python `str`, which reprs through [`py_str_repr`].
    Str(&'a str),
}

/// `repr(value)` for a Python scalar.
///
/// The bool arm comes first for the same reason it does in the Python: `bool`
/// is a subclass of `int` there, so an `isinstance(value, int)` test would
/// claim `True` and print it as `1`. Here the enum already separates them, and
/// the ordering is kept only so the two read the same way.
#[must_use]
pub fn py_repr(value: PyValue<'_>) -> String {
    match value {
        PyValue::Bool(true) => "True".to_string(),
        PyValue::Bool(false) => "False".to_string(),
        PyValue::Int(number) => number.to_string(),
        PyValue::Float(number) => py_float_repr(number),
        PyValue::Str(text) => py_str_repr(text),
    }
}

/// `repr(value)` for a Python `float`, byte for byte.
///
/// `CPython` builds this in `format_float_short()` (Python/pystrtod.c) with the
/// `'r'` format code, which is:
///
///   * the *shortest* digit string that reads back as the same double. Rust's
///     `{:e}` is the same guarantee from the other end -- shortest round-trip,
///     correctly rounded -- so the digits are taken from there rather than
///     reimplemented, and the work here is `CPython`'s layout around them.
///   * `decpt`, the position of the decimal point relative to the first
///     significant digit, decides the shape: `CPython` switches to exponential
///     when `decpt <= -4 || decpt > 16`. `{:e}` reports the exponent of the
///     leading digit, which is `decpt - 1`, so the same test reads
///     `exponent <= -5 || exponent >= 16`. That is why `1e15` prints in full
///     as `1000000000000000.0` and `1e16` prints as `1e+16`, and why `0.0001`
///     stays fixed while `9.999e-05` does not.
///   * `Py_DTSF_ADD_DOT_0`: a fixed-form result with nothing after the point
///     gets `.0` appended, so `1.0` is `1.0` and not `1`. Exponential form
///     never gets it -- `1e+16`, not `1.0e+16`.
///   * the exponent is always signed and always at least two digits, hence
///     `1e-05` and `1e+16` rather than `1e-5` and `1e16`. Wider exponents are
///     not padded further: `5e-324`.
///   * the sign is the sign *bit*, not the comparison, so negative zero prints
///     as `-0.0`.
///   * infinities and NaN are spelled `inf`, `-inf`, `nan`. NaN carries no
///     sign: `CPython` prints `nan` for a negative NaN too.
#[must_use]
pub fn py_float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    // is_sign_negative() rather than `value < 0.0`, so -0.0 keeps its sign.
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let shortest = format!("{:e}", value.abs());
    let Some((mantissa, exponent)) = shortest.split_once('e') else {
        // Unreachable: Rust's LowerExp always emits the marker. Falling back to
        // the raw formatting is still a number rather than a panic.
        return format!("{sign}{shortest}");
    };
    // Cannot fail -- LowerExp writes a decimal integer here -- and 0 would only
    // mis-place a decimal point rather than lose the digits.
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if exponent <= -5 || exponent >= 16 {
        format!(
            "{sign}{mantissa}e{}{:02}",
            exponent_sign(exponent),
            exponent.abs()
        )
    } else {
        let digits: String = mantissa.chars().filter(|ch| *ch != '.').collect();
        format!("{sign}{}", fixed_form(&digits, exponent + 1))
    }
}

/// `+` or `-`, because `CPython` always writes the exponent's sign.
fn exponent_sign(exponent: i32) -> char {
    if exponent < 0 {
        '-'
    } else {
        '+'
    }
}

/// The significant digits with a decimal point placed `decpt` digits in.
///
/// `decpt` is `CPython`'s: the value is `0.<digits> * 10**decpt`. Only the three
/// cases `CPython` distinguishes, and the `.0` of `Py_DTSF_ADD_DOT_0` in the last
/// one -- fixed form always has something on both sides of the point.
fn fixed_form(digits: &str, decpt: i32) -> String {
    let length = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    if decpt <= 0 {
        let zeros = "0".repeat(usize::try_from(-decpt).unwrap_or(0));
        format!("0.{zeros}{digits}")
    } else if decpt >= length {
        let zeros = "0".repeat(usize::try_from(decpt - length).unwrap_or(0));
        format!("{digits}{zeros}.0")
    } else {
        let (whole, fraction) = digits.split_at(usize::try_from(decpt).unwrap_or(0));
        format!("{whole}.{fraction}")
    }
}

/// `f"{value:g}"` for a Python `float`, byte for byte.
///
/// Two callers, both on the display path and both writing the result into a file a user or
/// a compositor reads: `render_displays()` spells a monitor's scale with it
/// (`lua_string(f'{float(item.get("scale", 1)):g}')`), and `display_snapshot()` spells the
/// refresh rate inside a mode string with it
/// (`f'{width}x{height}@{float(monitor.get("refreshRate", 60)):g}'`). `%g` rather than
/// `repr` is what turns a scale of `1.0` into `1` and a refresh rate of `143.998` into
/// `143.998` instead of `143.99800000000001`, which is the whole reason the Python reaches
/// for it.
///
/// The rules are C's `%g` with `CPython`'s defaults, and they are not [`py_float_repr`]'s:
///
///   * precision is 6 **significant** digits, and the value is rounded to that many before
///     anything decides which form to print. `1234567.0` is `1.23457e+06`, not `1234567`.
///   * the exponential form is taken when the decimal exponent of the rounded value is
///     below -4 or at least the precision -- so `0.0001` stays fixed while `1e-05` does
///     not, and `123456` stays fixed while `1234567` does not.
///   * trailing zeros in the fraction go, and so does a decimal point left with nothing
///     after it. `144.0` is `144`, `1.50` is `1.5`, and there is never a `.0` suffix --
///     which is exactly where this parts company with `repr`.
///   * the exponent is signed and at least two digits (`1e+20`, `1e-05`), as `repr`'s is.
///   * `inf`, `-inf` and `nan` are spelled as `CPython` spells them, and negative zero
///     keeps its sign (`-0`, not `-0.0`, since the fixed form drops the empty fraction).
#[must_use]
pub fn py_format_g(value: f64) -> String {
    /// `%g`'s default precision: six significant digits.
    const PRECISION: i32 = 6;

    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    // Rounded to `PRECISION` significant digits first, and read back off the scientific
    // form, because rounding is what decides the exponent: 999999.5 rounds to 1e+06 and
    // takes the exponential branch, while the unrounded value would have taken the fixed
    // one. Rust renormalizes the mantissa on that carry, exactly as C's `%e` does.
    let scientific = format!("{:.*e}", usize::try_from(PRECISION - 1).unwrap_or(0), value);
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        // Unreachable: Rust's LowerExp always emits the marker.
        return scientific;
    };
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if !(-4..PRECISION).contains(&exponent) {
        return format!(
            "{}e{}{:02}",
            trim_fraction(mantissa),
            exponent_sign(exponent),
            exponent.abs()
        );
    }
    // Fixed form, to as many decimals as leave `PRECISION` significant digits. Formatted
    // from the original value rather than assembled from the mantissa above, so the single
    // rounding is the one the formatter performs.
    let decimals = usize::try_from(PRECISION - 1 - exponent).unwrap_or(0);
    trim_fraction(&format!("{value:.decimals$}"))
}

/// Drop trailing zeros from a fixed-point spelling's fraction, and the point with them when
/// nothing is left after it. `%g`'s own trailing-zero removal, which `%e` and `%f` do not do.
fn trim_fraction(text: &str) -> String {
    if !text.contains('.') {
        return text.to_owned();
    }
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `repr(text)` for a Python `str`, byte for byte over ASCII and Latin-1.
///
/// `CPython`'s `unicode_repr()` (Objects/unicodeobject.c) picks the quote first
/// and escapes second:
///
///   * the quote is `'`, unless the string contains a `'` and no `"`, in which
///     case it is `"` -- so `it's` reprs as `"it's"` rather than `'it\'s'`,
///     and `both ' and "` goes back to `'` with the apostrophe escaped.
///   * a backslash and the chosen quote are backslash-escaped. The *other*
///     quote is not: it is only a character once it is not the delimiter.
///   * tab, newline and carriage return get their short escapes `\t \n \r`.
///     `CPython` has no short escape for the other C escapes here -- `\a` reprs
///     as `\x07`, not `\a`.
///   * anything else non-printable becomes `\xNN` with lowercase hex.
///
/// "Non-printable" is `CPython`'s `Py_UNICODE_ISPRINTABLE()`: not in the Unicode
/// categories Cc, Cf, Cs, Co, Cn, Zl, Zp or Zs, with space itself the one
/// exception. Below U+0100 that set is exactly [`is_unprintable_latin1`], which
/// is checked against `CPython` for all 256 code points in the tests.
///
/// **Byte-parity gap, stated plainly:** at and above U+0100 this function emits
/// the character verbatim. That matches `CPython` for every *printable* one --
/// which is nearly all of them, and all of the accented text a wallpaper path
/// or a locale name can hold -- and diverges for the non-printable ones
/// (U+200B, U+2028, unassigned code points, ...), where `CPython` would write
/// `​` and this writes the character. Closing that would mean shipping a
/// Unicode category table; no value this repr formats -- preference values
/// drawn from an enum, a number, a path or a colour -- can reach it.
#[must_use]
pub fn py_str_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ if ch == quote => {
                out.push('\\');
                out.push(ch);
            }
            // Writing into the buffer rather than allocating a `String` per
            // escape; a `fmt::Write` on a `String` cannot fail.
            _ if is_unprintable_latin1(ch) => {
                let _ = write!(out, "\\x{:02x}", ch as u32);
            }
            _ => out.push(ch),
        }
    }
    out.push(quote);
    out
}

/// Whether a code point below U+0100 is one `CPython` escapes rather than prints.
///
/// The C0 controls and DEL (Cc), the C1 controls U+0080..U+009F (Cc), the
/// no-break space U+00A0 (Zs, and Zs is unprintable apart from U+0020 itself),
/// and the soft hyphen U+00AD (Cf). Nothing else under U+0100 is escaped, so
/// `caf\u{e9}` reprs as `'café'`.
fn is_unprintable_latin1(ch: char) -> bool {
    matches!(ch, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{a0}' | '\u{ad}')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines of `<16 hex digits of the IEEE-754 bits> <repr>`, and of
    /// `<hex of the UTF-8 input> \t <repr>`, both generated by running
    /// python3 over a corpus once and checked in. The tests never shell out:
    /// a fixture is a record of what `CPython` said, and re-asking at test time
    /// would only make the suite depend on an interpreter being installed.
    const FLOATS: &str = include_str!("../testdata/float_repr.txt");
    const STRINGS: &str = include_str!("../testdata/py_repr.txt");

    fn rows(fixture: &str) -> impl Iterator<Item = (&str, &str)> {
        fixture
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .filter_map(|line| line.split_once([' ', '\t']))
    }

    #[test]
    fn floats_match_cpython_repr() {
        let mut count = 0;
        for (bits, expected) in rows(FLOATS) {
            let value = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
            assert_eq!(py_float_repr(value), expected, "bits {bits}");
            count += 1;
        }
        assert!(count >= 250, "corpus shrank to {count} floats");
    }

    #[test]
    fn floats_round_trip_through_the_repr() {
        for (bits, _) in rows(FLOATS) {
            let value = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
            let parsed: f64 = py_float_repr(value).parse().unwrap();
            assert_eq!(parsed.to_bits(), value.to_bits(), "bits {bits}");
        }
    }

    #[test]
    fn non_finite_floats_are_spelled_as_python_spells_them() {
        assert_eq!(py_float_repr(f64::NAN), "nan");
        assert_eq!(py_float_repr(-f64::NAN), "nan");
        assert_eq!(py_float_repr(f64::INFINITY), "inf");
        assert_eq!(py_float_repr(f64::NEG_INFINITY), "-inf");
    }

    #[test]
    fn the_documented_layout_switches_where_cpython_switches() {
        assert_eq!(py_float_repr(1e15), "1000000000000000.0");
        assert_eq!(py_float_repr(1e16), "1e+16");
        assert_eq!(py_float_repr(1e-4), "0.0001");
        assert_eq!(py_float_repr(9.999e-5), "9.999e-05");
        assert_eq!(py_float_repr(-0.0), "-0.0");
        assert_eq!(py_float_repr(0.1 + 0.2), "0.30000000000000004");
    }

    fn decode(hex: &str) -> String {
        let bytes: Vec<u8> = (0..hex.len() / 2)
            .filter_map(|index| hex.get(index * 2..index * 2 + 2))
            .filter_map(|pair| u8::from_str_radix(pair, 16).ok())
            .collect();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn strings_match_cpython_repr() {
        let mut count = 0;
        for (hex, expected) in rows(STRINGS) {
            let text = decode(hex);
            assert_eq!(py_str_repr(&text), expected, "input {hex}");
            count += 1;
        }
        assert!(count >= 40, "corpus shrank to {count} strings");
    }

    /// The empty string has no tab in the fixture, so it is checked by hand
    /// rather than through `rows()`, which needs a separator to split on.
    #[test]
    fn quote_choice_follows_cpython() {
        assert_eq!(py_str_repr(""), "''");
        assert_eq!(py_str_repr("it's"), "\"it's\"");
        assert_eq!(py_str_repr("say \"hi\""), "'say \"hi\"'");
        assert_eq!(py_str_repr("both ' and \""), "'both \\' and \"'");
    }

    /// Every code point under U+0100, against the set `CPython` escapes -- the
    /// table `is_unprintable_latin1` encodes, so a mistake in it fails here.
    #[test]
    fn latin1_printability_matches_cpython() {
        let escaped: Vec<u32> = (0u32..0x100)
            .filter(|point| char::from_u32(*point).is_some_and(is_unprintable_latin1))
            .collect();
        let expected: Vec<u32> = (0..=0x1f)
            .chain(0x7f..=0xa0)
            .chain(std::iter::once(0xad))
            .collect();
        assert_eq!(escaped, expected);
    }

    /// Every expectation here is `python3 -c "print(format(float(v), 'g'))"`, pasted in.
    #[test]
    fn the_general_format_is_cpythons_g() {
        // The two shapes the display path actually produces: a scale and a refresh rate.
        assert_eq!(py_format_g(1.0), "1");
        assert_eq!(py_format_g(1.5), "1.5");
        assert_eq!(py_format_g(1.25), "1.25");
        assert_eq!(py_format_g(59.996), "59.996");
        assert_eq!(py_format_g(143.998), "143.998");
        assert_eq!(py_format_g(144.0), "144");
        assert_eq!(py_format_g(60.0), "60");
        // Where the form switches, on both sides.
        assert_eq!(py_format_g(123_456.0), "123456");
        assert_eq!(py_format_g(1_234_567.0), "1.23457e+06");
        assert_eq!(py_format_g(1_234_560.0), "1.23456e+06");
        assert_eq!(py_format_g(0.0001), "0.0001");
        assert_eq!(py_format_g(1e-5), "1e-05");
        assert_eq!(py_format_g(1e20), "1e+20");
        assert_eq!(py_format_g(1e16), "1e+16");
        // Rounding is what decides the form: 999999.5 rounds up out of the fixed range.
        assert_eq!(py_format_g(999_999.5), "1e+06");
        // Six significant digits, and no more.
        assert_eq!(py_format_g(0.000_123_456_789), "0.000123457");
        assert_eq!(py_format_g(std::f64::consts::PI), "3.14159");
        assert_eq!(py_format_g(2.000_000_1), "2");
        // Zero, negative zero and the non-finite three.
        assert_eq!(py_format_g(0.0), "0");
        assert_eq!(py_format_g(-0.0), "-0");
        assert_eq!(py_format_g(-2.5), "-2.5");
        assert_eq!(py_format_g(f64::INFINITY), "inf");
        assert_eq!(py_format_g(f64::NEG_INFINITY), "-inf");
        assert_eq!(py_format_g(f64::NAN), "nan");
    }

    /// `%g` is not `repr`, and the display fragment depends on the difference: a scale of
    /// 1.0 has to reach Hyprland as `"1"`, not as `"1.0"`.
    #[test]
    fn the_general_format_drops_what_the_repr_keeps() {
        assert_eq!(py_float_repr(1.0), "1.0");
        assert_eq!(py_format_g(1.0), "1");
        assert_eq!(py_float_repr(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(py_format_g(0.1 + 0.2), "0.3");
    }

    #[test]
    fn scalars_repr_as_python_spells_them() {
        assert_eq!(py_repr(PyValue::Bool(true)), "True");
        assert_eq!(py_repr(PyValue::Bool(false)), "False");
        assert_eq!(py_repr(PyValue::Int(-43)), "-43");
        assert_eq!(py_repr(PyValue::Float(1.0)), "1.0");
        assert_eq!(py_repr(PyValue::Str("normal")), "'normal'");
    }
}
