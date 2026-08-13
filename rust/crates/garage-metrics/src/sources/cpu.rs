//! CPU: jiffie counters from `/proc/stat`, and the load averages beside them.

use crate::fault::Fault;
use crate::files::{as_float, parse_int, read_required, read_text};
use std::path::Path;

/// The two jiffie totals a CPU percentage is a delta between: everything, and the part
/// of everything that was idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Counters {
    /// Every jiffie in the line, idle included.
    pub(crate) total: i64,
    /// Idle plus iowait -- fields 4 and 5, which is what "not doing work" means here.
    pub(crate) idle: i64,
}

/// Aggregate jiffies and idle jiffies from `/proc/stat`'s first line.
///
/// Read directly rather than tolerantly: a machine with no `/proc/stat` is not a
/// machine this runs on, and the `OSError` is caught by the mode, which degrades the
/// widget to "n/a" with the errno in its tooltip.
pub(crate) fn counters() -> Result<Counters, Fault> {
    let text = read_required(Path::new("/proc/stat"))?;
    let first = text.lines().next().unwrap_or("");
    fields(first)
}

/// One `cpuN` line's two totals. `fields[3] + fields[4]` indexes past the `cpu` label,
/// so a truncated line is an `IndexError` -- which `bar_svg` catches and `stream` does
/// not, exactly as in the Python.
fn fields(line: &str) -> Result<Counters, Fault> {
    let mut total = 0_i64;
    let mut values = Vec::new();
    for field in line.split_whitespace().skip(1) {
        let value = parse_int(field)?;
        total += value;
        values.push(value);
    }
    let idle = match (values.get(3), values.get(4)) {
        (Some(idle), Some(iowait)) => idle + iowait,
        _ => return Err(Fault::index()),
    };
    Ok(Counters { total, idle })
}

/// The same two totals per core, for the popover's per-core bars.
///
/// `line[3:4].isdigit()` is the filter, which keeps `cpu0`..`cpuN` and drops both the
/// aggregate `cpu ` line and every other line in the file. An empty fourth character --
/// a line of exactly `cpu` -- is not a digit, so it is dropped too.
pub(crate) fn core_counters() -> Result<Vec<Counters>, Fault> {
    let text = read_required(Path::new("/proc/stat"))?;
    let mut cores = Vec::new();
    for line in text.lines() {
        let fourth = line.chars().nth(3);
        if !line.starts_with("cpu") || !fourth.is_some_and(|ch| ch.is_ascii_digit()) {
            continue;
        }
        cores.push(fields(line)?);
    }
    Ok(cores)
}

/// The busy share of the interval between two reads, as 0..100.
///
/// No previous counters means no interval, and no interval means no answer -- zero
/// rather than a guess. Same for a total that did not move, which is a second read
/// microseconds after the first.
pub(crate) fn percent(current: Counters, previous: Option<Counters>) -> f64 {
    let Some(previous) = previous else {
        return 0.0;
    };
    let total_delta = current.total - previous.total;
    let idle_delta = current.idle - previous.idle;
    if total_delta <= 0 {
        return 0.0;
    }
    (100.0 * (1.0 - as_float(idle_delta) / as_float(total_delta))).clamp(0.0, 100.0)
}

/// The three load averages, or nothing where `/proc/loadavg` is missing or unreadable.
///
/// Returns however many of the three the file actually had. The Python slices `[:3]`
/// without checking, so a short file gives a short list, and the caller that indexes
/// `load[2]` is the one that raises -- which is the behaviour, not a bug to fix here.
pub(crate) fn loadavg() -> Option<Vec<f64>> {
    let text = read_text(Path::new("/proc/loadavg"))?;
    if text.is_empty() {
        return None;
    }
    let mut values = Vec::new();
    for field in text.split_whitespace().take(3) {
        values.push(field.parse().ok()?);
    }
    Some(values)
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
    use super::{fields, percent, Counters};

    #[test]
    fn a_proc_stat_line_sums_every_field_and_picks_idle_plus_iowait() {
        let counters = fields("cpu  100 20 30 400 50 6 7 8 9 10").expect("parses");
        assert_eq!(
            counters.total,
            100 + 20 + 30 + 400 + 50 + 6 + 7 + 8 + 9 + 10
        );
        assert_eq!(counters.idle, 450);
    }

    #[test]
    fn a_truncated_line_is_an_index_error_not_a_zero() {
        assert_eq!(
            fields("cpu 1 2 3").expect_err("too short").to_string(),
            "list index out of range"
        );
    }

    #[test]
    fn a_non_numeric_field_is_a_value_error_quoting_the_field() {
        assert_eq!(
            fields("cpu 1 2 3 x 5")
                .expect_err("not a number")
                .to_string(),
            "invalid literal for int() with base 10: 'x'"
        );
    }

    #[test]
    fn the_first_read_of_a_session_reports_zero_rather_than_a_guess() {
        let now = Counters {
            total: 1000,
            idle: 900,
        };
        assert_eq!(percent(now, None), 0.0);
    }

    #[test]
    fn a_percentage_is_the_non_idle_share_of_the_interval() {
        let before = Counters {
            total: 1000,
            idle: 900,
        };
        let now = Counters {
            total: 1200,
            idle: 1050,
        };
        // 200 jiffies passed, 150 of them idle.
        assert_eq!(percent(now, Some(before)), 25.0);
    }

    #[test]
    fn a_stalled_or_reset_counter_reports_zero() {
        let before = Counters {
            total: 1000,
            idle: 900,
        };
        assert_eq!(percent(before, Some(before)), 0.0);
        let backwards = Counters { total: 10, idle: 5 };
        assert_eq!(percent(backwards, Some(before)), 0.0);
    }

    #[test]
    fn the_result_is_clamped_into_nought_to_a_hundred() {
        // An idle delta larger than the total delta happens when a core comes online
        // between two reads.
        let before = Counters {
            total: 1000,
            idle: 900,
        };
        let now = Counters {
            total: 1100,
            idle: 1200,
        };
        assert_eq!(percent(now, Some(before)), 0.0);
    }
}
