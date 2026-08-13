//! One clock reading rendered for reports, ledgers, logs, and backup directories.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch.
#[must_use]
pub fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0))
}

/// `time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime(seconds))`.
#[must_use]
pub fn local_iso8601(seconds: i64) -> String {
    let Ok(utc) = tz::UtcDateTime::from_timespec(seconds, 0) else {
        return String::new();
    };
    let local = tz::TimeZone::local()
        .ok()
        .and_then(|zone| utc.project(zone.as_ref()).ok());
    let (year, month, day, hour, minute, second, offset) = local.map_or_else(
        || utc_fields(&utc),
        |here| {
            (
                here.year(),
                here.month(),
                here.month_day(),
                here.hour(),
                here.minute(),
                here.second(),
                here.local_time_type().ut_offset(),
            )
        },
    );
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.abs() / 60;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{sign}{:02}{:02}",
        minutes / 60,
        minutes % 60
    )
}

/// `%Y%m%d-%H%M%S` from the same local-time fields as [`local_iso8601`].
#[must_use]
pub fn local_backup_stamp(seconds: i64) -> String {
    let stamp = local_iso8601(seconds);
    let kept: String = stamp
        .chars()
        .take(19)
        .filter(char::is_ascii_digit)
        .collect();
    let (date, time) = kept.split_at(kept.len().min(8));
    format!("{date}-{time}")
}

fn utc_fields(utc: &tz::UtcDateTime) -> (i32, u8, u8, u8, u8, u8, i32) {
    (
        utc.year(),
        utc.month(),
        utc.month_day(),
        utc.hour(),
        utc.minute(),
        utc.second(),
        0,
    )
}
