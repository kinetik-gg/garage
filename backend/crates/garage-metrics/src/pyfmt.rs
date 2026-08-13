//! The Python formatting behaviours this port has to reproduce byte for byte.
//!
//! Every number in a rendered strip is a string somebody reads: an SVG coordinate
//! Waybar rasterises, a throughput figure in a tooltip. The Python spells them with
//! f-string format specs and `str()`, and the two builds have to agree on every digit
//! or the SVG the Rust writes is a different file from the one the Python wrote.
//!
//! Three of the four specs the script uses need no help here. `{:.0f}`, `{:.1f}`,
//! `{:.2f}` and `{:.6f}` are `format!("{:.0}")` and friends: `CPython`'s
//! `float.__format__` and Rust's `Display for f64` both round the *exact* binary value
//! to the requested number of places, half to even, so they agree on the ties that
//! actually occur here -- a CPU percentage of exactly 74.5 prints `74` in both, and
//! 75.5 prints `76` in both. That was checked against `CPython` over four thousand
//! values covering every spec in the script; see the tests at the bottom.
//!
//! What is left is the three this module owns: the thousands separator in `{:,.2f}`,
//! which Rust has no spec for; `{:g}`, which Rust has no spec for either; and
//! `round(value, digits)`, which is not a formatting call at all but is the thing that
//! decides what the stream's numbers look like once `json.dumps` reprs them.
//!
//! `html.escape` lives here too, for the same reason: it is one of the two functions
//! standing between a stored string and the SVG.

/// `format(value, ",.<places>f")` -- a fixed-point number with the integer part grouped
/// into threes by commas.
///
/// `CPython` builds this in two stages and so does this: format the number to `places`
/// decimals first, then walk the integer digits inserting a comma every three from the
/// right. Doing it in that order is what makes `999.95` with two places come out as
/// `999.95` and with one place as `1,000.0` -- the rounding is what decides how many
/// integer digits there are to group, so grouping first would put the comma in the
/// wrong place on exactly the values that cross a power of a thousand.
///
/// The sign is peeled off before grouping and put back after, so a negative number
/// groups its digits rather than counting the `-` as one. Nothing in this script can
/// reach it with a negative -- every rate is floored at zero by [`crate::files::rate`]
/// -- but a grouping routine that mangles negatives is a trap for the next caller.
pub(crate) fn grouped(value: f64, places: usize) -> String {
    let formatted = format!("{value:.places$}");
    let (sign, rest) = match formatted.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", formatted.as_str()),
    };
    let (whole, fraction) = match rest.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (rest, None),
    };
    let mut out = String::with_capacity(formatted.len() + whole.len() / 3 + 1);
    out.push_str(sign);
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    if let Some(fraction) = fraction {
        out.push('.');
        out.push_str(fraction);
    }
    out
}

/// How many significant digits `{:g}` keeps. `CPython` and C both default to six.
const GENERAL_PRECISION: usize = 6;

/// `format(value, "g")` -- six significant digits, in whichever of fixed or exponential
/// notation is shorter, with the trailing zeros taken off.
///
/// The rule is C's `%g` and `CPython` inherits it: work out the decimal exponent of the
/// value *after* rounding to six significant digits, and use exponential form when that
/// exponent is below -4 or at least 6. Rounding first is load-bearing -- `999999.5` has
/// exponent 5 as written and 6 once rounded, and only the second answer produces
/// `1e+06` rather than `1000000`.
///
/// The only values that reach this are the icon box's `translate()` coordinates: an
/// integer x from the layout table and the constant 4.0 that centres a 14px glyph in a
/// 22px strip. All three print as bare integers, which is the whole reason the Python
/// spells them `{:g}` instead of `{}` -- `str(4.0)` would put `4.0` in the transform.
pub(crate) fn general(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    let scientific = format!("{value:.*e}", GENERAL_PRECISION - 1);
    let exponent: i32 = scientific
        .split_once('e')
        .and_then(|(_, exponent)| exponent.parse().ok())
        .unwrap_or(0);
    if exponent < -4 || exponent >= i32::try_from(GENERAL_PRECISION).unwrap_or(i32::MAX) {
        return exponential(&scientific, exponent);
    }
    let places =
        usize::try_from(i32::try_from(GENERAL_PRECISION).unwrap_or(0) - 1 - exponent).unwrap_or(0);
    strip_trailing_zeros(&format!("{value:.places$}"))
}

/// The exponential half of [`general`]: Rust writes `6.8e1`, `CPython` writes `6.8e+01`.
fn exponential(scientific: &str, exponent: i32) -> String {
    let mantissa = scientific
        .split_once('e')
        .map_or(scientific, |(head, _)| head);
    let sign = if exponent < 0 { '-' } else { '+' };
    format!(
        "{}e{sign}{:02}",
        strip_trailing_zeros(mantissa),
        exponent.abs()
    )
}

/// `%g`'s final pass: `4.0000` is `4`, `1.2500` is `1.25`, and an integer keeps no
/// decimal point at all. Only touches strings that already have a point, so an integer
/// that never grew one is returned whole.
fn strip_trailing_zeros(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `round(value, digits)` for a Python float, which is not the same operation as
/// `value.round()` and not the same as truncating a formatted string either.
///
/// `CPython`'s `float.__round__` (Objects/floatobject.c, `double_round`) rounds the
/// exact binary value to `digits` decimal places with `_Py_dg_dtoa` -- correctly, half
/// to even -- and then reads the decimal string back as a double. That round trip is
/// the definition, not an implementation detail: it is why `round(2.675, 2)` is
/// `2.67` and not `2.68`, because the double nearest 2.675 is a hair below it.
///
/// So this does exactly that, and leans on Rust's formatter for the hard half. Rust's
/// `{:.*}` is the same correctly-rounded, half-to-even conversion `_Py_dg_dtoa` gives,
/// which is what makes the two builds agree; the parse back is `strtod`, which both
/// languages share. Non-finite values are returned untouched, matching `CPython`'s
/// early return for them.
///
/// Every number in a `--stream` snapshot goes through here before `json.dumps` reprs
/// it, so a disagreement in this function is a disagreement in every line of the
/// stream.
pub(crate) fn py_round(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}").parse().unwrap_or(value)
}

/// `html.escape(text)` with its default `quote=True`.
///
/// Five substitutions in `CPython`'s own order, and the order matters: `&` is replaced
/// first so that the ampersands the other four introduce are not escaped again into
/// `&amp;lt;`. Both quote forms are covered because these strings land in an SVG
/// `<text>` element whose siblings carry double-quoted attributes.
///
/// Reachable input is the display and extra strings -- `42%`, `1.2M`, `n/a`, `--` --
/// none of which contain any of the five. It runs anyway because the value in the
/// state file is whatever the last tick wrote, and a device name is the one field here
/// a machine's hardware gets to choose the bytes of.
pub(crate) fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
// Byte-parity tests: a fixture row of the wrong shape is a broken fixture and panicking
// on it is the report, and a double that is only approximately the Python's is a failure
// rather than a pass -- so indexing and exact float comparison are both the point here.
#[allow(
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]
mod tests {
    use super::{general, grouped, html_escape, py_round};

    /// `<.0f>|<.1f>|<.2f>|<.6f>|<repr(round(v,1))>|<repr(round(v,2))>|<,.2f>` for each
    /// double in `testdata/float_formats.txt`, whose first field is the little-endian
    /// hex of the double itself. Generated by running `CPython` over a corpus of four
    /// thousand values -- percentages, throughputs, exact halves, exact quarters and
    /// uniformly random bit patterns -- and checked in, so the suite never needs an
    /// interpreter to run.
    const FORMATS: &str = include_str!("../testdata/float_formats.txt");

    fn corpus() -> impl Iterator<Item = (f64, Vec<&'static str>)> {
        FORMATS.lines().filter(|line| !line.is_empty()).map(|line| {
            let mut fields = line.split('|');
            let bits = fields.next().unwrap_or("0");
            let value = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
            (value, fields.collect())
        })
    }

    #[test]
    fn fixed_point_specs_match_cpython_on_every_value_in_the_corpus() {
        let mut checked = 0;
        for (value, expected) in corpus() {
            assert_eq!(format!("{value:.0}"), expected[0], "{value:?} at .0f");
            assert_eq!(format!("{value:.1}"), expected[1], "{value:?} at .1f");
            assert_eq!(format!("{value:.2}"), expected[2], "{value:?} at .2f");
            assert_eq!(format!("{value:.6}"), expected[3], "{value:?} at .6f");
            checked += 1;
        }
        assert!(checked >= 3000, "corpus shrank to {checked} values");
    }

    #[test]
    fn round_matches_cpython_on_every_value_in_the_corpus() {
        use garage_core::pyrepr::py_float_repr;
        for (value, expected) in corpus() {
            assert_eq!(py_float_repr(py_round(value, 1)), expected[4], "{value:?}");
            assert_eq!(py_float_repr(py_round(value, 2)), expected[5], "{value:?}");
        }
    }

    #[test]
    fn grouping_matches_cpython_on_every_value_in_the_corpus() {
        for (value, expected) in corpus() {
            assert_eq!(grouped(value, 2), expected[6], "{value:?}");
        }
    }

    #[test]
    fn ties_round_half_to_even_exactly_as_cpython_does() {
        // The four the CPU widget can actually hit: a percentage lands on a half
        // whenever the idle delta is an odd half of the total delta.
        assert_eq!(format!("{:.0}", 0.5_f64), "0");
        assert_eq!(format!("{:.0}", 1.5_f64), "2");
        assert_eq!(format!("{:.0}", 74.5_f64), "74");
        assert_eq!(format!("{:.0}", 75.5_f64), "76");
    }

    #[test]
    fn grouping_inserts_a_comma_every_three_integer_digits() {
        assert_eq!(grouped(0.0, 2), "0.00");
        assert_eq!(grouped(999.994, 2), "999.99");
        assert_eq!(grouped(1234.5, 2), "1,234.50");
        assert_eq!(grouped(1_234_567.891, 1), "1,234,567.9");
        assert_eq!(grouped(-1234.5, 2), "-1,234.50");
    }

    #[test]
    fn grouping_rounds_before_it_groups() {
        // 999.95 at one place crosses into four integer digits, so the comma has to be
        // placed against the rounded string rather than the original.
        assert_eq!(grouped(999.95, 1), "1,000.0");
    }

    #[test]
    fn general_prints_the_icon_transform_values_as_bare_integers() {
        assert_eq!(general(0.0), "0");
        assert_eq!(general(4.0), "4");
        assert_eq!(general(68.0), "68");
    }

    #[test]
    fn general_follows_percent_g_outside_the_values_this_script_uses() {
        assert_eq!(general(-0.0), "-0");
        assert_eq!(general(0.675), "0.675");
        assert_eq!(general(123_456.789), "123457");
        assert_eq!(general(1e15), "1e+15");
        assert_eq!(general(1e-5), "1e-05");
        assert_eq!(general(0.0001), "0.0001");
        assert_eq!(general(f64::INFINITY), "inf");
        assert_eq!(general(f64::NAN), "nan");
    }

    #[test]
    fn round_reproduces_the_cases_that_make_it_not_arithmetic() {
        assert_eq!(py_round(2.675, 2), 2.67);
        assert_eq!(py_round(1.005, 2), 1.0);
        assert_eq!(py_round(99.995, 2), 100.0);
        assert!(py_round(f64::NAN, 2).is_nan());
    }

    #[test]
    fn html_escape_covers_all_five_substitutions_without_double_escaping() {
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("<b>"), "&lt;b&gt;");
        assert_eq!(html_escape("\"q\""), "&quot;q&quot;");
        assert_eq!(html_escape("it's"), "it&#x27;s");
        assert_eq!(html_escape("&<>"), "&amp;&lt;&gt;");
    }

    #[test]
    fn html_escape_leaves_the_strings_a_strip_actually_carries_alone() {
        for text in ["42%", "n/a", "--", "1.2M", "24.0G", "\u{2193}999.9M"] {
            assert_eq!(html_escape(text), text);
        }
    }
}
