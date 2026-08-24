//! Python-compatible numeric formatting retained by the metrics stream.

/// `round(value, digits)` for a Python float.
///
/// Formatting to the requested precision and parsing back reproduces the exact
/// binary-to-decimal round trip used by `CPython` for the finite sensor values that
/// reach the stream. Non-finite values pass through unchanged.
pub(crate) fn py_round(value: f64, digits: usize) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.digits$}").parse().unwrap_or(value)
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::indexing_slicing)]
mod tests {
    use super::py_round;

    /// `<.0f>|<.1f>|<.2f>|<.6f>|<repr(round(v,1))>|<repr(round(v,2))>|<,.2f>`.
    const FORMATS: &str = include_str!("../testdata/float_formats.txt");

    #[test]
    fn round_matches_cpython_on_every_value_in_the_corpus() {
        use garage_core::pyrepr::py_float_repr;

        for line in FORMATS.lines().filter(|line| !line.is_empty()) {
            let mut fields = line.split('|');
            let bits = fields.next().unwrap_or("0");
            let value = f64::from_bits(u64::from_str_radix(bits, 16).unwrap_or(0));
            let expected: Vec<_> = fields.collect();
            assert_eq!(py_float_repr(py_round(value, 1)), expected[4], "{value:?}");
            assert_eq!(py_float_repr(py_round(value, 2)), expected[5], "{value:?}");
        }
    }
}
