//! What the coercion pass says when it puts a value back, byte for byte.
//!
//! `validate_preferences()` builds one note per substitution:
//!
//! ```text
//! f"{section}.{key} {config[section].get(key)!r} is not valid, using {default!r}"
//! ```
//!
//! and hands the list to `report_preference_notes()`, which either prints
//! `garage: preferences.toml: <note>` on stderr -- the journal under the render
//! units -- or extends a caller's sink. `garage doctor` counts that sink, so a
//! note that reads differently here is a note the docs and the tests no longer
//! describe. The prefix belongs to the reporting half; a [`Notes`] entry is the
//! bare string, exactly as the Python's `notes` list holds it.

use crate::pyrepr::{py_repr, PyValue};
use crate::schema::prefs::PreferenceKey;

/// The notes one pass produced, in the order the pass produced them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    notes: Vec<String>,
}

impl Notes {
    /// An empty list, which is what a file this build can render costs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one substitution: `keep()`'s note, with both values repr'd.
    pub fn push_coercion(
        &mut self,
        key: PreferenceKey,
        value: &toml::Value,
        default: &toml::Value,
    ) {
        self.notes.push(format!(
            "{key} {} is not valid, using {}",
            py_repr_toml(value),
            py_repr_toml(default)
        ));
    }

    /// The notes, in order.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.notes
    }

    /// How many substitutions the pass made -- what `garage doctor` reports.
    #[must_use]
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// Whether the stored file needed no correction at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

/// `repr(value)` for a value `tomllib` would have parsed out of the file.
///
/// The four scalars go through [`py_repr`]. An array and a table are here
/// because a hand-edited file can carry one where a scalar belongs, and that is
/// precisely a value the pass reports: `tomllib` gives Python a `list` and a
/// `dict`, whose reprs are the bracket and brace forms below.
#[must_use]
pub fn py_repr_toml(value: &toml::Value) -> String {
    match value {
        toml::Value::Boolean(flag) => py_repr(PyValue::Bool(*flag)),
        toml::Value::Integer(number) => py_repr(PyValue::Int(*number)),
        toml::Value::Float(number) => py_repr(PyValue::Float(*number)),
        toml::Value::String(text) => py_repr(PyValue::Str(text)),
        toml::Value::Datetime(stamp) => py_repr_datetime(stamp),
        toml::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(py_repr_toml).collect();
            format!("[{}]", parts.join(", "))
        }
        toml::Value::Table(entries) => {
            let parts: Vec<String> = entries
                .iter()
                .map(|(key, item)| {
                    format!("{}: {}", py_repr(PyValue::Str(key)), py_repr_toml(item))
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// `str(value)` for the same, which is `repr` for everything but a string.
///
/// `wallpaper_target()` reads the two unchecked paths as `str(value or "")`, so
/// this is the other half of that expression: Python stringifies a `str` as
/// itself and everything else as its repr.
#[must_use]
pub fn py_str_toml(value: &toml::Value) -> String {
    value
        .as_str()
        .map_or_else(|| py_repr_toml(value), ToString::to_string)
}

/// `repr()` of what `tomllib` builds for a TOML date, time or datetime.
///
/// Reachable from a hand edit and from nothing else: `theme_light_at =
/// 07:00:00` is a TOML local time, not the string the schema wants, so the note
/// that puts it back has to spell a `datetime.time` the way Python does --
/// positional arguments, trailing zero components dropped.
///
/// **Parity gap, stated plainly:** an offset-carrying datetime gets its
/// `tzinfo=` argument from [`py_repr_offset`], which spells `datetime.timezone`
/// exactly for UTC and for whole-minute offsets, the only ones TOML can write.
fn py_repr_datetime(stamp: &toml::value::Datetime) -> String {
    let (date, time) = (stamp.date, stamp.time);
    let offset = stamp.offset.map(py_repr_offset).unwrap_or_default();
    match (date, time) {
        (Some(date), None) => {
            format!("datetime.date({}, {}, {})", date.year, date.month, date.day)
        }
        (None, Some(time)) => format!("datetime.time({})", py_time_arguments(&time)),
        (Some(date), Some(time)) => format!(
            "datetime.datetime({}, {}, {}, {}{offset})",
            date.year,
            date.month,
            date.day,
            py_time_arguments(&time)
        ),
        // Not constructible: `toml_datetime` rejects a stamp with neither half.
        (None, None) => "datetime.datetime(1, 1, 1, 0, 0)".to_string(),
    }
}

/// `hour, minute[, second[, microsecond]]`, dropping trailing zeros the way
/// `datetime`'s repr does -- but never the minute, which it always prints.
fn py_time_arguments(time: &toml::value::Time) -> String {
    // Both halves are optional in the parser (TOML 1.1 lets a time stop at the
    // minute); absent reads as zero, which is what `tomllib` fills in too.
    let second = time.second.unwrap_or(0);
    let microsecond = time.nanosecond.unwrap_or(0) / 1_000;
    let (hour, minute) = (time.hour, time.minute);
    if microsecond > 0 {
        format!("{hour}, {minute}, {second}, {microsecond}")
    } else if second > 0 {
        format!("{hour}, {minute}, {second}")
    } else {
        format!("{hour}, {minute}")
    }
}

/// The `tzinfo=` argument of an offset-carrying datetime.
///
/// `datetime.timezone.utc` is a singleton with a repr of its own; any other
/// offset reprs as the constructor over a `timedelta`, which normalizes to a
/// non-negative seconds count under a possibly negative day.
fn py_repr_offset(offset: toml::value::Offset) -> String {
    let minutes = match offset {
        toml::value::Offset::Z => return ", tzinfo=datetime.timezone.utc".to_string(),
        toml::value::Offset::Custom { minutes } => i64::from(minutes),
    };
    if minutes == 0 {
        return ", tzinfo=datetime.timezone.utc".to_string();
    }
    let seconds = minutes * 60;
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let arguments = if days == 0 {
        format!("seconds={rest}")
    } else {
        format!("days={days}, seconds={rest}")
    };
    format!(", tzinfo=datetime.timezone(datetime.timedelta({arguments}))")
}

#[cfg(test)]
mod tests {
    use super::{py_repr_toml, py_str_toml, Notes};
    use crate::schema::prefs::PreferenceKey;

    fn parse(text: &str) -> toml::Value {
        let table: toml::Table = format!("value = {text}").parse().unwrap();
        table.get("value").unwrap().clone()
    }

    #[test]
    fn the_note_is_the_python_f_string() {
        let mut notes = Notes::new();
        notes.push_coercion(
            PreferenceKey::CornerRadius,
            &toml::Value::String("huge".to_string()),
            &toml::Value::String("normal".to_string()),
        );
        assert_eq!(
            notes.as_slice(),
            ["appearance.corner_radius 'huge' is not valid, using 'normal'"]
        );
        assert_eq!(notes.len(), 1);
        assert!(!notes.is_empty());
    }

    #[test]
    fn scalars_repr_as_python_reprs_them() {
        assert_eq!(py_repr_toml(&parse("true")), "True");
        assert_eq!(py_repr_toml(&parse("43")), "43");
        assert_eq!(py_repr_toml(&parse("1.0")), "1.0");
        assert_eq!(
            py_repr_toml(&parse("0.30000000000000004")),
            "0.30000000000000004"
        );
        assert_eq!(py_repr_toml(&parse("\"it's\"")), "\"it's\"");
    }

    /// Checked against `python3 -c "import tomllib; print(repr(...))"` before
    /// being written down here.
    #[test]
    fn containers_repr_as_list_and_dict() {
        assert_eq!(py_repr_toml(&parse("[1, \"a\"]")), "[1, 'a']");
        assert_eq!(
            py_repr_toml(&parse("{a = 1, b = \"c\"}")),
            "{'a': 1, 'b': 'c'}"
        );
        assert_eq!(py_repr_toml(&parse("[]")), "[]");
    }

    #[test]
    fn datetimes_repr_as_the_datetime_module_spells_them() {
        assert_eq!(
            py_repr_toml(&parse("1979-05-27")),
            "datetime.date(1979, 5, 27)"
        );
        assert_eq!(py_repr_toml(&parse("07:00:00")), "datetime.time(7, 0)");
        assert_eq!(py_repr_toml(&parse("07:32:45")), "datetime.time(7, 32, 45)");
        assert_eq!(
            py_repr_toml(&parse("07:32:00.123456")),
            "datetime.time(7, 32, 0, 123456)"
        );
        assert_eq!(
            py_repr_toml(&parse("1979-05-27T07:32:00")),
            "datetime.datetime(1979, 5, 27, 7, 32)"
        );
        assert_eq!(
            py_repr_toml(&parse("1979-05-27T07:32:00Z")),
            "datetime.datetime(1979, 5, 27, 7, 32, tzinfo=datetime.timezone.utc)"
        );
        assert_eq!(
            py_repr_toml(&parse("1979-05-27T07:32:00-01:00")),
            "datetime.datetime(1979, 5, 27, 7, 32, \
             tzinfo=datetime.timezone(datetime.timedelta(days=-1, seconds=82800)))"
        );
    }

    #[test]
    fn str_differs_from_repr_only_for_a_string() {
        assert_eq!(py_str_toml(&parse("\"~/pic.png\"")), "~/pic.png");
        assert_eq!(py_str_toml(&parse("true")), "True");
        assert_eq!(py_str_toml(&parse("[1]")), "[1]");
    }
}
