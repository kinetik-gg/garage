//! The value a display record is made of, and the two shapes that hold them.
//!
//! `displays.toml` is TOML and the Displays pane's `display-test` payload is JSON, and the
//! same record travels between them: the pane sends a layout, `display_finish()` writes it to
//! the file, and the next session reads it back. Python has one `dict` for both because both
//! parsers produce plain `dict`/`list`/scalars. This module is that one shape, named -- plus
//! the handful of `str()`, `int()`, `float()` and truthiness readings every consumer of a
//! record performs on it.

use garage_core::pyrepr::py_float_repr;
use thiserror::Error;

/// One value out of a display record, in the two shapes it can arrive in.
///
/// `displays.toml` is TOML and the Displays pane's `display-test` payload is JSON, and the
/// same record travels between them: the pane sends a layout, `display_finish()` writes it to
/// the file, and the next session reads it back. Python has one `dict` for both because both
/// parsers produce plain `dict`/`list`/scalars; this is that one shape, named.
///
/// Not `toml::Value` and not `serde_json::Value`: TOML has a datetime and no null, JSON has a
/// null and no datetime, and a renderer that named the JSON type would put `serde_json` on
/// this crate's dependency edge for a value it never parses. Each side converts into this on
/// the way in -- [`LayoutValue::from_toml`] here, and `garage-apply`'s own converter for the
/// wire format.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutValue {
    /// JSON `null`, which the pane can send for any field. TOML cannot hold one.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// An integer. TOML's own, or a JSON number with no fraction.
    Int(i64),
    /// A float.
    Float(f64),
    /// A string.
    Str(String),
    /// A list -- `availableModes` is the one the snapshot carries.
    Array(Vec<LayoutValue>),
    /// A nested object. Nothing in a display record is one today; kept so a payload that
    /// contains one round-trips rather than being silently flattened.
    Table(Vec<(String, LayoutValue)>),
    /// A TOML datetime, which only a hand-edited `displays.toml` can produce. Kept whole,
    /// with its source spelling, because `str()` of one is what would reach a message.
    Datetime(String),
}

/// `int()` or `float()` refusing a value a hand-edited `displays.toml` -- or a pane sending
/// the wrong type -- put where a number belongs.
///
/// The text is `CPython`'s own, because it reaches the user through the envelope's `error`
/// field: `main()` catches `ValueError` beside `SettingsError` and prints `str(error)`.
///
/// **Fidelity boundary, stated plainly:** Python raises `ValueError` for a string that does
/// not parse and `TypeError` for a value of the wrong *kind* (`None`, a list, a dict), and
/// only the first of those is caught by `main()` -- a `TypeError` leaves a traceback on
/// stderr and exit 1 rather than an envelope. Both arrive here as one error, so the wrong-kind
/// cases reach the user as a well-formed envelope carrying `CPython`'s `TypeError` wording
/// instead of a traceback. The message is the same; the shape around it is better.
#[derive(Debug, Clone, Error)]
#[error("{0}")]
pub struct NumberError(pub(crate) String);

/// A mirror the apply path has to refuse: `SettingsError(f"{output} {problem}")`.
#[derive(Debug, Clone, Error)]
#[error("{0}")]
pub struct MirrorRefusal(pub(crate) String);

impl LayoutValue {
    /// One TOML value as a layout value. A datetime keeps its source spelling; everything
    /// else maps across unchanged.
    #[must_use]
    pub fn from_toml(value: &toml::Value) -> Self {
        match value {
            toml::Value::Boolean(flag) => Self::Bool(*flag),
            toml::Value::Integer(number) => Self::Int(*number),
            toml::Value::Float(number) => Self::Float(*number),
            toml::Value::String(text) => Self::Str(text.clone()),
            toml::Value::Datetime(stamp) => Self::Datetime(stamp.to_string()),
            toml::Value::Array(items) => Self::Array(items.iter().map(Self::from_toml).collect()),
            toml::Value::Table(table) => Self::Table(
                table
                    .iter()
                    .map(|(key, held)| (key.clone(), Self::from_toml(held)))
                    .collect(),
            ),
        }
    }

    /// Python's truthiness: every empty thing is false, `None` is false, and every non-empty
    /// container, non-zero number and non-empty string is true.
    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(flag) => *flag,
            Self::Int(number) => *number != 0,
            Self::Float(number) => *number != 0.0,
            Self::Str(text) => !text.is_empty(),
            Self::Array(items) => !items.is_empty(),
            Self::Table(entries) => !entries.is_empty(),
            // A datetime is an object, and every object is truthy.
            Self::Datetime(_) => true,
        }
    }

    /// `str(value)`, for the keys the layout reads through one -- `output`, `mode`, `mirror`,
    /// `primary`.
    #[must_use]
    pub fn py_str(&self) -> String {
        match self {
            Self::Null => "None".to_owned(),
            Self::Bool(true) => "True".to_owned(),
            Self::Bool(false) => "False".to_owned(),
            Self::Int(number) => number.to_string(),
            Self::Float(number) => py_float_repr(*number),
            Self::Str(text) => text.clone(),
            Self::Datetime(stamp) => stamp.clone(),
            // `str([...])` and `str({...})` are the repr, which for the shapes reachable
            // here is close enough to be worth spelling rather than refusing: nothing in a
            // display record is a container where a string belongs.
            Self::Array(items) => {
                let parts: Vec<String> = items.iter().map(Self::py_str).collect();
                format!("[{}]", parts.join(", "))
            }
            Self::Table(entries) => {
                let parts: Vec<String> = entries
                    .iter()
                    .map(|(key, held)| format!("'{key}': {}", held.py_str()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
        }
    }

    /// `float(value)`.
    ///
    /// # Errors
    ///
    /// [`NumberError`] with `CPython`'s own wording for a string that does not parse or a
    /// value of a kind `float()` refuses.
    #[allow(
        clippy::cast_precision_loss,
        reason = "`float(int)` in CPython is this same round-to-nearest-double conversion"
    )]
    pub fn py_float(&self) -> Result<f64, NumberError> {
        match self {
            Self::Bool(flag) => Ok(if *flag { 1.0 } else { 0.0 }),
            Self::Int(number) => Ok(*number as f64),
            Self::Float(number) => Ok(*number),
            // `float("  1.5 ")` strips surrounding whitespace and accepts `inf`/`nan`; the
            // underscore separators it also accepts are not spelled here, since nothing
            // writes one into a display record.
            Self::Str(text) => text.trim().parse::<f64>().map_err(|_| {
                NumberError(format!(
                    "could not convert string to float: {}",
                    garage_core::pyrepr::py_str_repr(text)
                ))
            }),
            other @ (Self::Null | Self::Array(_) | Self::Table(_) | Self::Datetime(_)) => {
                Err(NumberError(format!(
                    "float() argument must be a string or a real number, not '{}'",
                    other.py_type_name()
                )))
            }
        }
    }

    /// `int(value)`: truncating toward zero for a float, and strict for a string.
    ///
    /// # Errors
    ///
    /// [`NumberError`], as [`LayoutValue::py_float`].
    #[allow(
        clippy::cast_possible_truncation,
        reason = "`int(float)` truncates toward zero, which is exactly this cast; the \
                  non-finite inputs CPython refuses are turned away above it, and a \
                  magnitude past i64 saturates where CPython would grow a bignum -- a \
                  coordinate no display could hold"
    )]
    pub fn py_int(&self) -> Result<i64, NumberError> {
        match self {
            Self::Bool(flag) => Ok(i64::from(*flag)),
            Self::Int(number) => Ok(*number),
            // `int(float('nan'))` is a ValueError and `int(float('inf'))` an OverflowError;
            // both are spelled as CPython spells them.
            Self::Float(number) if number.is_nan() => Err(NumberError(
                "cannot convert float NaN to integer".to_owned(),
            )),
            Self::Float(number) if number.is_infinite() => Err(NumberError(
                "cannot convert float infinity to integer".to_owned(),
            )),
            // `int(2.9)` is 2 and `int(-2.9)` is -2: toward zero, not toward minus infinity.
            Self::Float(number) => Ok(*number as i64),
            // `int("1.5")` is a ValueError -- unlike `float`, the string form is integers
            // only, with surrounding whitespace and a sign allowed.
            Self::Str(text) => text.trim().parse::<i64>().map_err(|_| {
                NumberError(format!(
                    "invalid literal for int() with base 10: {}",
                    garage_core::pyrepr::py_str_repr(text)
                ))
            }),
            other @ (Self::Null | Self::Array(_) | Self::Table(_) | Self::Datetime(_)) => {
                Err(NumberError(format!(
                    "int() argument must be a string, a bytes-like object or a real \
                     number, not '{}'",
                    other.py_type_name()
                )))
            }
        }
    }

    /// `type(value).__name__`, for the two `TypeError` messages above.
    fn py_type_name(&self) -> &'static str {
        match self {
            Self::Null => "NoneType",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "str",
            Self::Array(_) => "list",
            Self::Table(_) => "dict",
            Self::Datetime(_) => "datetime.datetime",
        }
    }
}

/// One `[[display]]` record: an ordered mapping, exactly as the Python's `dict` is.
///
/// Ordered rather than sorted because the pending-transaction file is written by serialising
/// one of these, and a map that reordered the keys would rewrite a record the pane sent.
/// Nothing *reads* a record positionally -- every consumer asks for a key by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayEntry {
    fields: Vec<(String, LayoutValue)>,
}

impl DisplayEntry {
    /// A record from its fields, in the order they were given.
    #[must_use]
    pub fn from_fields(fields: Vec<(String, LayoutValue)>) -> Self {
        Self { fields }
    }

    /// Every field, in insertion order.
    #[must_use]
    pub fn fields(&self) -> &[(String, LayoutValue)] {
        &self.fields
    }

    /// `item.get(key)`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&LayoutValue> {
        self.fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, held)| held)
    }

    /// `item[key] = value`: an existing key keeps its position, a new one is appended.
    pub fn set(&mut self, key: &str, value: LayoutValue) {
        match self.fields.iter_mut().find(|(name, _)| name == key) {
            Some(existing) => existing.1 = value,
            None => self.fields.push((key.to_owned(), value)),
        }
    }

    /// `str(item.get("output", ""))`, which is how every caller here names a display.
    #[must_use]
    pub fn output(&self) -> String {
        self.get("output")
            .map_or_else(String::new, LayoutValue::py_str)
    }

    /// `item.get("enabled", True)`, at Python's truthiness: only an explicitly false-y value
    /// turns a display off.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.get("enabled").is_none_or(LayoutValue::truthy)
    }

    /// `str(item.get("mirror") or "")`: a false-y mirror is no mirror, not its spelling.
    #[must_use]
    pub fn mirror(&self) -> String {
        match self.get("mirror") {
            Some(held) if held.truthy() => held.py_str(),
            _ => String::new(),
        }
    }
}

/// A whole saved layout: which display is primary, and the records themselves.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayLayout {
    /// `str(raw.get("primary", ""))`.
    pub primary: String,
    /// `raw.get("display", [])`, which the rest of the code calls `displays`.
    pub displays: Vec<DisplayEntry>,
}
