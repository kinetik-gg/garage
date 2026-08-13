//! The tolerant primitives every sensor is built on, and the three pure numeric
//! helpers that turn a counter delta into something a strip can draw.
//!
//! Every source in this binary is optional hardware. A missing file is the normal case
//! on somebody else's machine, not an error, so the readers here return `None` and the
//! callers decide what a missing number means for their metric. Nothing in this crate
//! may raise its way out to a traceback in the bar.
//!
//! The distinction to keep straight is between these readers and the direct reads a few
//! sensors do. [`read_text`] swallows the failure; `Path("/proc/stat").read_text()` in
//! the Python does not, and its `OSError` is caught much further out by the mode. Where
//! the Python reads a path directly, this crate uses [`read_required`], which turns the
//! same failure into a [`Fault`] carrying `CPython`'s own message.

use crate::fault::Fault;
use std::fs;
use std::path::Path;

/// A mebibyte, the unit every throughput figure in the strips is quoted in.
pub(crate) const MIB: f64 = 1_048_576.0;

/// The same constant where the Python is multiplying integers rather than dividing
/// floats -- `int(parts[2]) * MIB` on nvidia-smi's MiB figures, which stays exact.
pub(crate) const MIB_BYTES: i64 = 1_048_576;

/// The ceiling of the log scale that disk and network throughput are drawn against, in
/// MiB/s. Linear axes are useless here: idle is 0.01 MiB/s and an `NVMe` burst is 3000, so
/// a linear graph is a flat line with occasional spikes to the top. 2 GiB/s as the top
/// of the scale keeps both readable.
const LOG_CEILING_MIB: f64 = 2048.0;

/// `read_text(path)` -- the file's contents stripped, or nothing.
///
/// Python's `str.strip()` with no argument removes whitespace at both ends; Rust's
/// `trim` removes the Unicode `White_Space` set, which is a superset by a handful of
/// characters no `/sys` attribute contains.
pub(crate) fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

/// `read_int(path)` -- the file as a decimal integer, or nothing.
///
/// Both of Python's failure modes collapse to `None` here as they do there: no text at
/// all is its `TypeError`, and text that is not a number is its `ValueError`.
pub(crate) fn read_int(path: &Path) -> Option<i64> {
    read_text(path).and_then(|text| text.parse().ok())
}

/// A file the caller reads directly rather than tolerantly, so its failure becomes a
/// [`Fault`] the mode will catch -- `/proc/stat` and `/proc/meminfo`, the two files
/// whose absence means the machine is not a Linux box at all.
///
/// Not stripped, because both callers split the whole text into lines and a trailing
/// newline is already invisible to that.
pub(crate) fn read_required(path: &Path) -> Result<String, Fault> {
    fs::read_to_string(path).map_err(|error| Fault::errno(path, &error))
}

/// `int(text)` where a failure is a [`Fault`] rather than a `None`.
pub(crate) fn parse_int(text: &str) -> Result<i64, Fault> {
    text.parse().map_err(|_| Fault::bad_int(text))
}

/// Every entry a glob would have matched, sorted by full path.
///
/// The Python spells this `sorted(Path(root).glob(pattern))` and the `sorted` is doing
/// real work: `glob` yields in `readdir` order, which is the filesystem's whim, so two
/// runs on one machine can disagree about which `hwmon` is examined first. Sorting by
/// the whole path string is what `PurePath` comparison amounts to for a set of paths
/// that differ in one component, which is every glob in this crate -- and it means
/// `hwmon10` sorts before `hwmon2`, exactly as it does in the Python.
///
/// A directory that cannot be read is an empty list rather than an error, which is what
/// `glob` gives for a missing root.
pub(crate) fn sorted_children(root: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    paths
}

/// An integer counter as the float the arithmetic that follows wants.
///
/// Every division in this crate is a Python `/`, which on two ints produces a float, so
/// this conversion happens at exactly the point `CPython` would do it internally -- and
/// `CPython`'s int-to-double conversion is the same round-to-nearest one, so both builds
/// lose the same bits past 2^53. Nothing that reaches here is near that: a jiffie
/// counter, a byte count and a sector count are all comfortably under it.
#[expect(
    clippy::cast_precision_loss,
    reason = "this is Python's own int-to-float conversion, at the same point and with \
              the same rounding; every counter that reaches it is far below 2^53"
)]
pub(crate) fn as_float(value: i64) -> f64 {
    value as f64
}

/// A count as a float, for the two places a length becomes part of a coordinate.
#[expect(
    clippy::cast_precision_loss,
    reason = "a bucket length and a history length are bounded by POINTS, which is 120"
)]
pub(crate) fn count_as_float(value: usize) -> f64 {
    value as f64
}

/// `int(value)` -- truncated toward zero, which is what Python's `int()` does to a float
/// and what `as` does to one that is already in range.
#[expect(
    clippy::cast_possible_truncation,
    reason = "`as` saturates at i64's bounds rather than wrapping, and the two callers \
              pass a graph width and a counter read back out of a state file"
)]
pub(crate) fn as_int(value: f64) -> i64 {
    value as i64
}

/// Per-second delta, floored at zero.
///
/// Counters reset when an interface goes down or a device is re-plugged, and a reset
/// read as a delta is a spectacular negative spike. Clamping is the only honest answer:
/// the rate over an interval that contains a reset is unknown, and zero is closer than
/// minus four gigabytes.
pub(crate) fn rate(current: Option<i64>, previous: Option<i64>, elapsed: f64) -> f64 {
    let (Some(current), Some(previous)) = (current, previous) else {
        return 0.0;
    };
    if elapsed <= 0.0 {
        return 0.0;
    }
    // `max(0, current - previous)` in Python clamps the *integer* delta and only then
    // divides, so the numerator is exact. Same order here.
    as_float(current.saturating_sub(previous).max(0)) / elapsed
}

/// Throughput as 0..100 on the log curve the old disk graph used.
pub(crate) fn log_scale(mib_per_second: f64) -> f64 {
    (mib_per_second.max(0.0).ln_1p() / LOG_CEILING_MIB.ln_1p() * 100.0).min(100.0)
}

/// A throughput figure in the four characters a strip can spare for it.
pub(crate) fn compact_rate(mib_per_second: f64) -> String {
    if mib_per_second >= 1024.0 {
        return format!("{:.1}G", mib_per_second / 1024.0);
    }
    if mib_per_second >= 1.0 {
        return format!("{mib_per_second:.1}M");
    }
    format!("{:.0}K", mib_per_second * 1024.0)
}

/// How much time a graph covers, for the tail of the tooltip.
pub(crate) fn compact_span(seconds: f64) -> String {
    if seconds < 90.0 {
        return format!("{seconds:.0}s");
    }
    format!("{:.1} min", seconds / 60.0)
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
    use super::{
        compact_rate, compact_span, log_scale, rate, read_int, read_text, sorted_children,
    };
    use crate::scratch::Scratch;
    use std::fs;
    use std::path::Path;

    #[test]
    fn a_missing_file_is_none_rather_than_an_error() {
        assert_eq!(read_text(Path::new("/no/such/garage-metrics-file")), None);
        assert_eq!(read_int(Path::new("/no/such/garage-metrics-file")), None);
    }

    #[test]
    fn a_sysfs_reading_is_stripped_of_its_newline() {
        let scratch = Scratch::new("metrics-read");
        let path = scratch.join("temp1_input");
        fs::write(&path, "68750\n").expect("write");
        assert_eq!(read_text(&path).as_deref(), Some("68750"));
        assert_eq!(read_int(&path), Some(68750));
    }

    #[test]
    fn text_that_is_not_a_number_reads_as_none() {
        let scratch = Scratch::new("metrics-read-bad");
        let path = scratch.join("name");
        fs::write(&path, "k10temp\n").expect("write");
        assert_eq!(read_int(&path), None);
    }

    #[test]
    fn children_sort_lexically_so_hwmon10_comes_before_hwmon2() {
        let scratch = Scratch::new("metrics-glob");
        for name in ["hwmon2", "hwmon10", "hwmon1"] {
            fs::create_dir_all(scratch.join(name)).expect("mkdir");
        }
        let names: Vec<String> = sorted_children(scratch.path())
            .iter()
            .filter_map(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(names, ["hwmon1", "hwmon10", "hwmon2"]);
    }

    #[test]
    fn a_missing_directory_globs_to_nothing() {
        assert!(sorted_children(Path::new("/no/such/garage-metrics-dir")).is_empty());
    }

    #[test]
    fn a_counter_that_went_backwards_rates_as_zero_rather_than_negative() {
        assert_eq!(rate(Some(10), Some(400), 2.0), 0.0);
        assert_eq!(rate(Some(400), Some(10), 2.0), 195.0);
        assert_eq!(rate(None, Some(10), 2.0), 0.0);
        assert_eq!(rate(Some(10), None, 2.0), 0.0);
        assert_eq!(rate(Some(400), Some(10), 0.0), 0.0);
        assert_eq!(rate(Some(400), Some(10), -1.0), 0.0);
    }

    #[test]
    fn the_log_scale_keeps_idle_and_a_burst_both_on_the_graph() {
        assert_eq!(log_scale(0.0), 0.0);
        assert_eq!(log_scale(-5.0), 0.0);
        assert_eq!(log_scale(2048.0), 100.0);
        assert_eq!(log_scale(100_000.0), 100.0);
        // 1 MiB/s is well up the curve where a linear axis would have it invisible.
        assert!(log_scale(1.0) > 8.0 && log_scale(1.0) < 10.0);
    }

    #[test]
    fn compact_rate_switches_unit_at_the_thresholds_the_python_uses() {
        assert_eq!(compact_rate(0.0), "0K");
        assert_eq!(compact_rate(0.5), "512K");
        assert_eq!(compact_rate(0.999), "1023K");
        assert_eq!(compact_rate(1.0), "1.0M");
        assert_eq!(compact_rate(999.94), "999.9M");
        assert_eq!(compact_rate(1023.9), "1023.9M");
        assert_eq!(compact_rate(1024.0), "1.0G");
        assert_eq!(compact_rate(24_576.0), "24.0G");
    }

    #[test]
    fn compact_span_switches_to_minutes_at_ninety_seconds() {
        assert_eq!(compact_span(0.0), "0s");
        assert_eq!(compact_span(89.9), "90s");
        assert_eq!(compact_span(90.0), "1.5 min");
        assert_eq!(compact_span(240.0), "4.0 min");
    }
}
