//! `provider()`, `metric()`, `percent()`, `reset()`, `reset_days()`: pulling one number or
//! one string out of a tokscale usage entry for the bar tooltip. [`crate::cache::valid`] is
//! the other half of the Python's data-shaping section; it lives next to the cache it
//! guards instead.

use serde_json::Value;

use crate::timeutil;

/// `provider(payload, name)`: the first entry whose `"provider"` is `name`, or an empty
/// object -- never absent, so every caller here can keep reading fields off the result with
/// plain `.get()` the way the Python reads off `{}`.
pub(crate) fn provider(payload: &Value, name: &str) -> Value {
    payload
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("provider").and_then(Value::as_str) == Some(name))
        })
        .cloned()
        .unwrap_or_else(empty_object)
}

/// `metric(item, label)`: the entry in `item["metrics"]` whose `"label"` is `label`, or an
/// empty object.
fn metric(item: &Value, label: &str) -> Value {
    item.get("metrics")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|entry| entry.get("label").and_then(Value::as_str) == Some(label))
        })
        .cloned()
        .unwrap_or_else(empty_object)
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// `percent(item, label)`: `remaining_percent`, rounded to the nearest whole percent, or
/// `"\u{2014}"` (em dash) when it is missing or not a number.
///
/// The Python's `isinstance(value, (int, float))` also accepts `bool` -- `bool` is an `int`
/// subclass in Python, so a stray JSON `true`/`false` there would format as `"1%"` /
/// `"0%"`. Tokscale has no reason to ever emit a boolean for a percentage, and this port
/// does not special-case it: a `Value::Bool` here reads as "not a number" and falls to the
/// dash, a deliberate, reported deviation rather than a reproduced quirk.
pub(crate) fn percent(item: &Value, label: &str) -> String {
    match metric(item, label)
        .get("remaining_percent")
        .and_then(Value::as_f64)
    {
        Some(number) => format!("{number:.0}%"),
        None => "\u{2014}".to_string(),
    }
}

/// `reset(item, label)`: `resets_at`, converted to `Asia/Jakarta` and formatted as
/// `"%Y-%m-%d %H:%M WIB"`; `"not reported"` when it is missing or empty; the raw string,
/// unparsed, when it does not parse as an ISO 8601 timestamp -- matching the Python's
/// `except (ValueError, KeyError): return value`.
pub(crate) fn reset(item: &Value, label: &str) -> String {
    let Some(value) = resets_at(item, label) else {
        return "not reported".to_string();
    };
    match timeutil::parse_iso8601_utc_seconds(&value) {
        Some(epoch) => timeutil::format_wib(epoch),
        None => value,
    }
}

/// `reset_days(item, label)`: whole days from `now` until `resets_at`, rounded up, floored
/// at zero, formatted `"{n}D"`; `"\u{2014}"` when `resets_at` is missing, empty, or does
/// not parse.
///
/// `now` is a parameter rather than read from the clock in here, so the arithmetic can be
/// exercised against a fixed instant in tests; [`crate::output`] is what supplies the real
/// wall clock.
pub(crate) fn reset_days(item: &Value, label: &str, now_epoch_seconds: f64) -> String {
    let Some(value) = resets_at(item, label) else {
        return "\u{2014}".to_string();
    };
    match timeutil::parse_iso8601_utc_seconds(&value) {
        Some(epoch) => {
            let days = ((epoch - now_epoch_seconds) / 86_400.0).ceil();
            let days = if days.is_finite() { days.max(0.0) } else { 0.0 };
            // `days` is a small non-negative whole number (weeks-to-months of seconds
            // divided by a day, ceiling'd) -- always far inside `i64`.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = days as i64;
            format!("{whole}D")
        }
        None => "\u{2014}".to_string(),
    }
}

/// The shared first half of `reset()` and `reset_days()`: `resets_at`, or `None` when it is
/// absent or an empty string -- matching the Python's `if not value`. Owned, rather than
/// borrowed from the [`Value`] `metric()` builds: that value is a temporary (`metric()`
/// clones its match out of `item`), so nothing here can hold a reference into it.
fn resets_at(item: &Value, label: &str) -> Option<String> {
    let value = metric(item, label)
        .get("resets_at")
        .and_then(Value::as_str)
        .map(str::to_owned)?;
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{metric, percent, provider, reset, reset_days};
    use serde_json::json;

    fn sample_payload() -> serde_json::Value {
        json!([
            {
                "provider": "Codex",
                "plan": "pro",
                "metrics": [
                    {"label": "Weekly", "remaining_percent": 87.6, "resets_at": "2024-06-08T00:00:00Z"},
                    {"label": "Session", "remaining_percent": 42.4}
                ]
            },
            {
                "provider": "Claude",
                "plan": "max",
                "metrics": [
                    {"label": "Weekly", "remaining_percent": 5.0, "resets_at": "not-a-timestamp"}
                ]
            }
        ])
    }

    #[test]
    fn provider_finds_by_name_and_defaults_to_empty() {
        let payload = sample_payload();
        assert_eq!(provider(&payload, "Codex").get("plan"), Some(&json!("pro")));
        assert_eq!(provider(&payload, "Missing"), json!({}));
        assert_eq!(provider(&json!([]), "Codex"), json!({}));
    }

    #[test]
    fn metric_finds_by_label_and_defaults_to_empty() {
        let codex = provider(&sample_payload(), "Codex");
        assert_eq!(
            metric(&codex, "Weekly").get("remaining_percent"),
            Some(&json!(87.6))
        );
        assert_eq!(metric(&codex, "Missing"), json!({}));
    }

    #[test]
    fn percent_rounds_to_the_nearest_whole_number() {
        let payload = sample_payload();
        let codex = provider(&payload, "Codex");
        assert_eq!(percent(&codex, "Weekly"), "88%");
        assert_eq!(percent(&codex, "Session"), "42%");
        assert_eq!(percent(&codex, "Missing"), "\u{2014}");
    }

    #[test]
    fn percent_rejects_non_numeric_values() {
        let item = json!({"metrics": [{"label": "Weekly", "remaining_percent": "87"}]});
        assert_eq!(percent(&item, "Weekly"), "\u{2014}");
    }

    #[test]
    fn reset_formats_in_wib() {
        let payload = sample_payload();
        let codex = provider(&payload, "Codex");
        // 2024-06-08T00:00:00Z is 2024-06-08 07:00 WIB.
        assert_eq!(reset(&codex, "Weekly"), "2024-06-08 07:00 WIB");
    }

    #[test]
    fn reset_falls_back_to_the_raw_string_when_unparseable() {
        let payload = sample_payload();
        let claude = provider(&payload, "Claude");
        assert_eq!(reset(&claude, "Weekly"), "not-a-timestamp");
    }

    #[test]
    fn reset_reports_not_reported_when_absent() {
        let item = json!({"metrics": []});
        assert_eq!(reset(&item, "Weekly"), "not reported");
    }

    fn fixture_seconds(iso: &str) -> f64 {
        crate::timeutil::parse_iso8601_utc_seconds(iso).expect("fixture timestamp parses")
    }

    #[test]
    fn reset_days_rounds_up_and_floors_at_zero() {
        let item = json!({
            "metrics": [{"label": "Weekly", "resets_at": "2024-06-08T00:00:00Z"}]
        });
        // Exactly 6 days before the reset instant: still 6 whole days, not 5.
        let six_days_before = fixture_seconds("2024-06-02T00:00:00Z");
        assert_eq!(reset_days(&item, "Weekly", six_days_before), "6D");

        // One second before the reset instant: rounds up to 1 day, not 0.
        let one_second_before = fixture_seconds("2024-06-08T00:00:00Z") - 1.0;
        assert_eq!(reset_days(&item, "Weekly", one_second_before), "1D");

        // Past the reset instant: clamped to zero, not negative.
        let one_day_after = fixture_seconds("2024-06-09T00:00:00Z");
        assert_eq!(reset_days(&item, "Weekly", one_day_after), "0D");
    }

    #[test]
    fn reset_days_is_a_dash_when_unparseable_or_absent() {
        let item = json!({"metrics": [{"label": "Weekly", "resets_at": "garbage"}]});
        assert_eq!(reset_days(&item, "Weekly", 0.0), "\u{2014}");
        assert_eq!(reset_days(&json!({}), "Weekly", 0.0), "\u{2014}");
    }
}
