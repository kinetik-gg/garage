//! Calendar arithmetic for `resets_at` timestamps, ported from the Python's `datetime` /
//! `zoneinfo` use in `reset()` and `reset_days()`.
//!
//! No timezone database. The Python converts a `resets_at` instant to `Asia/Jakarta` (WIB)
//! before formatting it, via `ZoneInfo("Asia/Jakarta")`. That zone has carried a fixed
//! UTC+7 offset with no daylight-saving transitions since Indonesia's 1932/1988
//! standardisation of Western Indonesia Time, and every timestamp this module ever sees is
//! within days of "now" (2026) -- nowhere near that boundary. So the conversion here is a
//! constant seven-hour shift ([`WIB_OFFSET_SECONDS`]), not a zone-table lookup, and `std`
//! alone is enough: no `chrono-tz` / `jiff` dependency was added for this crate.
//!
//! The other half is parsing: `datetime.fromisoformat(value.replace("Z", "+00:00"))`. The
//! calendar side of that (`civil_from_days` / `days_from_civil`) is Howard Hinnant's
//! well-known proleptic-Gregorian <-> day-count algorithm
//! (<https://howardhinnant.github.io/date_algorithms.html>), correct for every date this
//! module can be handed and not worth a crate on its own.

/// Days since the Unix epoch (1970-01-01) for a proleptic-Gregorian `(year, month, day)`.
/// Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let month = i64::from(month);
    let day = i64::from(day);
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = shifted_year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The inverse of [`days_from_civil`]: a day count since the Unix epoch back to
/// `(year, month, day)`. Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    // `month_prime` is derived from a `day_of_year` in 0..=365 and is always in 0..=11, so
    // `month` is always in 1..=12 and `day` in 1..=31: both fit `u32` with no loss.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    (year, month as u32, day as u32)
}

/// Seven hours, in seconds: the whole of what converting to `Asia/Jakarta` (WIB) does to a
/// timestamp near "now" -- see the module docs for why no zone database is needed for that.
const WIB_OFFSET_SECONDS: i64 = 7 * 3600;

/// Parse a `resets_at` string -- ISO 8601, `Z` or a numeric UTC offset, optional fractional
/// seconds -- into seconds since the Unix epoch (UTC), fraction included.
///
/// Mirrors `datetime.fromisoformat(value.replace("Z", "+00:00"))` followed by "assume UTC
/// if the parse left the result naive". Deliberately more lenient than one thing the Python
/// does implicitly: a `resets_at` that is present but not a string at all would raise
/// `AttributeError` out of `value.replace(...)` in the Python -- uncaught by `reset()`'s
/// `except (ValueError, KeyError)` -- and crash the whole script. That is treated here as
/// an ordinary unparseable value (`None`), not reproduced as a crash; see the crate's report
/// for this deviation.
pub(crate) fn parse_iso8601_utc_seconds(raw: &str) -> Option<f64> {
    let normalised = raw.replace('Z', "+00:00");
    let (date_part, rest) = normalised.split_once(['T', ' '])?;

    let (year, month, day) = parse_date(date_part)?;
    let (time_part, offset_part) = split_time_offset(rest);
    let (hour, minute, second, fraction) = parse_time(time_part)?;
    let offset_seconds = parse_offset(offset_part)?;

    let days = days_from_civil(year, month, day);
    let naive = days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);
    let utc_seconds = naive - offset_seconds;
    // Epoch seconds for any date within a few centuries of 1970 stay far inside 2^53; no
    // precision is lost converting them to `f64` to add the sub-second fraction back in.
    #[allow(clippy::cast_precision_loss)]
    Some(utc_seconds as f64 + fraction)
}

fn parse_date(text: &str) -> Option<(i64, u32, u32)> {
    let mut parts = text.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

/// Split a time-and-offset remainder such as `"12:34:56.789+07:00"` into the time part and
/// the offset part (including its sign), or `(text, "")` when there is no offset.
fn split_time_offset(text: &str) -> (&str, &str) {
    match text.find(['+', '-']) {
        // `+`/`-` are single ASCII bytes, so `index` always lands on a char boundary.
        Some(index) => text.split_at(index),
        None => (text, ""),
    }
}

fn parse_time(text: &str) -> Option<(u32, u32, u32, f64)> {
    let mut parts = text.splitn(3, ':');
    let hour: u32 = parts.next()?.parse().ok()?;
    let minute: u32 = parts.next()?.parse().ok()?;
    let second_field = parts.next()?;
    let (second_text, fraction) = match second_field.split_once('.') {
        Some((whole, frac_digits)) if !frac_digits.is_empty() => {
            let fraction: f64 = format!("0.{frac_digits}").parse().ok()?;
            (whole, fraction)
        }
        Some((whole, _)) => (whole, 0.0),
        None => (second_field, 0.0),
    };
    let second: u32 = second_text.parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some((hour, minute, second, fraction))
}

/// Parse a `+HH:MM` / `-HH:MM` / `+HHMM` / `+HH` offset into signed seconds east of UTC.
/// Empty input (no offset present) is UTC, matching the Python's "naive -> assume UTC".
fn parse_offset(text: &str) -> Option<i64> {
    if text.is_empty() {
        return Some(0);
    }
    let (sign_char, rest) = text.split_at(1);
    let sign: i64 = match sign_char {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let digits: String = rest.chars().filter(|c| *c != ':').collect();
    let hour_str = digits.get(0..2)?;
    let minute_str = digits.get(2..4).unwrap_or("00");
    let hour: i64 = hour_str.parse().ok()?;
    let minute: i64 = minute_str.parse().ok()?;
    Some(sign * (hour * 3600 + minute * 60))
}

/// Format a UTC instant (seconds since the epoch, with fraction) as `reset()` does:
/// `"%Y-%m-%d %H:%M WIB"` after shifting to `Asia/Jakarta`. Fractional seconds cannot cross
/// a minute boundary and are discarded, exactly as `strftime("%H:%M")` discards them.
pub(crate) fn format_wib(epoch_utc_seconds: f64) -> String {
    // Only the whole-second part ever reaches the calendar math; see the doc comment above.
    #[allow(clippy::cast_possible_truncation)]
    let whole_seconds = epoch_utc_seconds.floor() as i64;
    let local = whole_seconds + WIB_OFFSET_SECONDS;
    let days = local.div_euclid(86_400);
    let seconds_of_day = local.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} WIB")
}

/// Format a UTC instant as `datetime.now(timezone.utc).isoformat()` does: `+00:00`, never
/// `Z`, and the `.ffffff` microsecond group present only when it is non-zero -- Python's
/// `isoformat()` omits it entirely otherwise.
pub(crate) fn format_fetched_at_utc(epoch_seconds: i64, microseconds: u32) -> String {
    let days = epoch_seconds.div_euclid(86_400);
    let seconds_of_day = epoch_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    if microseconds == 0 {
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}+00:00")
    } else {
        format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{microseconds:06}+00:00"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        civil_from_days, days_from_civil, format_fetched_at_utc, format_wib,
        parse_iso8601_utc_seconds,
    };

    #[test]
    fn epoch_is_day_zero() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn known_dates_match_known_day_counts() {
        // 2000-03-01 is a well-known checkpoint in Hinnant's own test suite.
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        // The day before a (Gregorian) leap-year February 29th.
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }

    #[test]
    fn civil_and_days_round_trip_across_a_wide_range() {
        for days in (-800_000..800_000).step_by(4001) {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days, "day {days} -> {y}-{m}-{d}");
        }
    }

    // Test fixtures only: the expected epoch values are small (single-digit years' worth of
    // seconds), nowhere near the 2^53 boundary where `i64 -> f64` would actually lose a bit.
    #[allow(clippy::cast_precision_loss)]
    #[test]
    fn parses_z_suffix_as_utc() {
        let seconds = parse_iso8601_utc_seconds("2024-06-01T12:34:56Z").expect("parses");
        // 2024-06-01 is day 19875 since epoch (from days_from_civil), at 12:34:56.
        let expected = days_from_civil(2024, 6, 1) * 86_400 + 12 * 3600 + 34 * 60 + 56;
        assert!((seconds - expected as f64).abs() < 1e-9);
    }

    #[allow(clippy::cast_precision_loss)]
    #[test]
    fn parses_explicit_offset_and_fraction() {
        let seconds = parse_iso8601_utc_seconds("2024-06-01T19:34:56.5+07:00").expect("parses");
        let expected = days_from_civil(2024, 6, 1) * 86_400 + 12 * 3600 + 34 * 60 + 56;
        assert!((seconds - (expected as f64 + 0.5)).abs() < 1e-9);
    }

    #[test]
    fn naive_timestamp_is_assumed_utc() {
        let with_z = parse_iso8601_utc_seconds("2024-06-01T12:34:56Z").expect("parses");
        let naive = parse_iso8601_utc_seconds("2024-06-01T12:34:56").expect("parses");
        assert!((with_z - naive).abs() < 1e-9);
    }

    #[test]
    fn garbage_does_not_parse() {
        assert_eq!(parse_iso8601_utc_seconds("not a timestamp"), None);
        assert_eq!(parse_iso8601_utc_seconds(""), None);
    }

    #[allow(clippy::cast_precision_loss)]
    #[test]
    fn wib_is_seven_hours_ahead_of_utc() {
        // Midnight UTC on 2024-06-01 is 07:00 WIB the same day.
        let midnight_utc = (days_from_civil(2024, 6, 1) * 86_400) as f64;
        assert_eq!(format_wib(midnight_utc), "2024-06-01 07:00 WIB");
    }

    #[allow(clippy::cast_precision_loss)]
    #[test]
    fn wib_carries_across_a_day_boundary() {
        // 18:00 UTC on 2024-06-01 is 01:00 WIB on 2024-06-02.
        let evening_utc = (days_from_civil(2024, 6, 1) * 86_400 + 18 * 3600) as f64;
        assert_eq!(format_wib(evening_utc), "2024-06-02 01:00 WIB");
    }

    #[test]
    fn fetched_at_omits_microseconds_when_zero() {
        let epoch = days_from_civil(2024, 6, 1) * 86_400 + 12 * 3600;
        assert_eq!(format_fetched_at_utc(epoch, 0), "2024-06-01T12:00:00+00:00");
        assert_eq!(
            format_fetched_at_utc(epoch, 250_000),
            "2024-06-01T12:00:00.250000+00:00"
        );
    }
}
