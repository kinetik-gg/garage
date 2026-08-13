//! Memory: the totals from `/proc/meminfo`, and the PSI stall share beside them.

use crate::fault::Fault;
use crate::files::{as_float, parse_int, read_required, read_text};
use std::path::Path;

/// What one read of `/proc/meminfo` says about memory, in bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Memory {
    /// Total less available -- what is actually spoken for.
    pub(crate) used: i64,
    /// `MemTotal`.
    pub(crate) total: i64,
    /// `MemAvailable`, or `MemFree` on a kernel too old to publish the first.
    pub(crate) available: i64,
    /// Used as a share of total, 0..100.
    pub(crate) percent: f64,
}

/// One read of `/proc/meminfo`, folded into the four figures the widget and the popover
/// both want.
///
/// `MemAvailable` rather than `MemFree` wherever the kernel offers it: free memory on a
/// box with a warm page cache is close to zero and says nothing, while available is the
/// kernel's own estimate of what a new allocation could actually get.
///
/// Every value in the file is in kibibytes and is multiplied up here, so nothing
/// downstream has to remember which unit it is holding.
pub(crate) fn values() -> Result<Memory, Fault> {
    let text = read_required(Path::new("/proc/meminfo"))?;
    let mut total = None;
    let mut available = None;
    let mut free = None;
    for line in text.lines() {
        let (key, value) = parse_line(line)?;
        match key {
            "MemTotal" => total = Some(value),
            "MemAvailable" => available = Some(value),
            "MemFree" => free = Some(value),
            _ => (),
        }
    }
    // `values["MemTotal"]` is a direct subscript in the Python: a file without the key
    // is a KeyError, which bar_svg degrades and stream does not catch.
    let total = total.ok_or_else(|| Fault::key("MemTotal"))?;
    let available = available.or(free).unwrap_or(0);
    Ok(Memory {
        used: total - available,
        total,
        available,
        percent: if total == 0 {
            0.0
        } else {
            as_float(total - available) / as_float(total) * 100.0
        },
    })
}

/// One `Key:   1234 kB` line as its key and its value in bytes.
///
/// Both failures the Python can hit are here: a line with no colon does not unpack into
/// two halves (`ValueError`), and a line whose value is not a number is an `int()`
/// failure (also `ValueError`). A line with a key and nothing after it indexes past the
/// end of the split (`IndexError`).
fn parse_line(line: &str) -> Result<(&str, i64), Fault> {
    let (key, rest) = line.split_once(':').ok_or_else(|| Fault::bad_unpack(1))?;
    let number = rest.split_whitespace().next().ok_or_else(Fault::index)?;
    Ok((key, parse_int(number)? * 1024))
}

/// PSI "some" avg10 for memory, or nothing where PSI is not compiled in.
///
/// "some" rather than "full": some is the share of time at least one task was stalled
/// waiting on memory, which is the number that moves before anything is visibly wrong.
/// full only moves once nothing can run at all.
pub(crate) fn pressure() -> Option<f64> {
    let text = read_text(Path::new("/proc/pressure/memory"))?;
    if text.is_empty() {
        return None;
    }
    for line in text.lines() {
        if !line.starts_with("some") {
            continue;
        }
        for field in line.split_whitespace().skip(1) {
            let (key, value) = field.split_once('=').unwrap_or((field, ""));
            if key == "avg10" {
                // A malformed avg10 ends the search rather than continuing it, matching
                // the Python's `return None` inside the loop.
                return value.parse().ok();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_line;

    #[test]
    fn a_meminfo_line_comes_back_in_bytes() {
        assert_eq!(
            parse_line("MemTotal:       63311800 kB"),
            Ok(("MemTotal", 63_311_800 * 1024))
        );
    }

    #[test]
    fn a_line_with_no_unit_still_parses() {
        assert_eq!(
            parse_line("HugePages_Total:       0"),
            Ok(("HugePages_Total", 0))
        );
    }

    #[test]
    fn a_line_with_no_colon_is_the_unpack_failure_python_reports() {
        assert_eq!(
            parse_line("garbage").expect_err("no colon").to_string(),
            "not enough values to unpack (expected 2, got 1)"
        );
    }

    #[test]
    fn a_line_with_nothing_after_the_colon_is_an_index_error() {
        assert_eq!(
            parse_line("MemTotal:").expect_err("no value").to_string(),
            "list index out of range"
        );
    }

    #[test]
    fn a_non_numeric_value_quotes_itself_in_the_error() {
        assert_eq!(
            parse_line("MemTotal: lots kB")
                .expect_err("not a number")
                .to_string(),
            "invalid literal for int() with base 10: 'lots'"
        );
    }
}
