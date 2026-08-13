//! The JSON value the state files and the stream are made of, in Python's shape.
//!
//! `serde_json` is the obvious answer and it is the wrong one here, for two reasons
//! that both come down to bytes on disk.
//!
//! The first is ordering. The state file is `json.dumps(state)` of a plain dict, and a
//! Python dict is insertion-ordered, so the order the keys come out in is the order the
//! script put them in: whatever the previous tick's file held, then `period`, then
//! `history`, then the sample's own fields, then `last_sample`. A `BTreeMap` sorts them
//! and a `HashMap` shuffles them, and either way the file the Rust writes stops
//! comparing equal to the file the Python writes. [`Object`] is therefore an ordered
//! association list with Python's `dict` semantics: assigning to a key that already
//! exists leaves it where it was, and only a genuinely new key moves to the end.
//!
//! The second is number spelling. `json.dumps` writes a float with `float.__repr__` --
//! shortest round-trip, `1e+16` with a signed two-digit exponent, `-0.0` keeping its
//! sign -- and an int with no decimal point at all. That distinction is visible in the
//! state file: [`crate::state::mark_unavailable`] primes a history with Python's `int`
//! zero, so an unavailable widget's file holds `[0, 0, ...]` while a sampled one holds
//! `[0.0, 0.0, ...]`. A value type that folded both into one numeric kind would write
//! the wrong one back. Hence [`Value::Int`] and [`Value::Float`] as separate arms, with
//! the repr taken from `garage_core::pyrepr`.
//!
//! Everything else in here is the smallest parser and encoder that covers what the
//! Python's `json` module accepts and emits for these two files, and no more.

mod dump;
mod parse;

pub(crate) use dump::dumps;
pub(crate) use parse::loads;

use garage_core::pyrepr::py_float_repr;

/// One JSON value, in the Python types `json.loads` produces.
///
/// `Int` and `Float` are separate because `json` keeps them separate in both
/// directions: `0` parses to `int` and `0.0` to `float`, and each reprs back the way it
/// came. See the module docs for where that distinction is load-bearing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    /// `null`, which is Python's `None`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// A JSON number with no fraction and no exponent -- Python's `int`.
    Int(i64),
    /// A JSON number with a fraction or an exponent -- Python's `float`. Also the
    /// non-finite literals `NaN`, `Infinity` and `-Infinity`, which Python's `json`
    /// both accepts and emits by default.
    Float(f64),
    /// A JSON string.
    Str(String),
    /// A JSON array -- Python's `list`.
    List(Vec<Value>),
    /// A JSON object -- Python's `dict`, insertion-ordered.
    Object(Object),
}

impl Value {
    /// A string [`Value`], from anything that can become a `String`.
    pub(crate) fn str(text: impl Into<String>) -> Self {
        Self::Str(text.into())
    }

    /// A list [`Value`] of strings, which is what every `tooltip_parts` is.
    pub(crate) fn strings<I: IntoIterator<Item = S>, S: Into<String>>(parts: I) -> Self {
        Self::List(parts.into_iter().map(Self::str).collect())
    }

    /// Python's truth value.
    ///
    /// Used wherever the script writes a bare `if` over something out of the state
    /// file -- `if period`, `state.get("active")`, `state.get("history") or [...]` --
    /// and the answer has to be Python's rather than "is it present". Zero, the empty
    /// string and the empty container are all false, and `0.0` being false is what
    /// makes a smoothed `period` of exactly zero leave the span off the tooltip.
    pub(crate) fn is_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(flag) => *flag,
            Self::Int(number) => *number != 0,
            Self::Float(number) => *number != 0.0,
            Self::Str(text) => !text.is_empty(),
            Self::List(items) => !items.is_empty(),
            Self::Object(fields) => !fields.is_empty(),
        }
    }

    /// `str(value)` for the values that reach one, which is Python's `str` and not its
    /// `repr`: a string is itself, unquoted.
    ///
    /// Two callers, and both of them put the result somewhere a person reads. The
    /// tooltip joins `tooltip_parts` with `str(part)`, and the strip's `<text>` is
    /// `html.escape(str(state.get("display", "--")))`. Both fields are written as
    /// strings by every code path in this crate, so the interesting arms are the ones
    /// a hand-edited or a half-written state file can produce: `None` spells `None`,
    /// a bool spells `True`/`False`, and a float goes through `repr`, which is what
    /// `str` is for a Python float.
    ///
    /// Containers get the shape `str(list)` and `str(dict)` give, which is their
    /// `repr` -- with the caveat that the elements inside use `str` rather than
    /// `repr`, so a nested string is unquoted where `CPython` would quote it. No
    /// caller can reach a container here (a `tooltip_parts` element or a `display` is
    /// a scalar in every branch of `sample_widget`), and spelling one at all is
    /// only so that a corrupt file produces a readable tooltip rather than nothing.
    pub(crate) fn py_str(&self) -> String {
        match self {
            Self::Null => "None".to_string(),
            Self::Bool(true) => "True".to_string(),
            Self::Bool(false) => "False".to_string(),
            Self::Int(number) => number.to_string(),
            Self::Float(number) => py_float_repr(*number),
            Self::Str(text) => text.clone(),
            Self::List(items) => joined(items, "[", "]"),
            Self::Object(fields) => joined(&fields.values(), "{", "}"),
        }
    }

    /// The value as a number, when it already is one. `None` for everything else,
    /// which is the caller's cue to take Python's `TypeError` branch.
    pub(crate) fn as_number(&self) -> Option<f64> {
        match self {
            Self::Int(number) => Some(crate::files::as_float(*number)),
            Self::Float(number) => Some(*number),
            Self::Bool(flag) => Some(f64::from(u8::from(*flag))),
            Self::Null | Self::Str(_) | Self::List(_) | Self::Object(_) => None,
        }
    }

    /// The value as a list, when it is one.
    pub(crate) fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(items) => Some(items),
            Self::Null
            | Self::Bool(_)
            | Self::Int(_)
            | Self::Float(_)
            | Self::Str(_)
            | Self::Object(_) => None,
        }
    }

    /// The value as an object, when it is one -- `isinstance(state, dict)`.
    pub(crate) fn into_object(self) -> Option<Object> {
        match self {
            Self::Object(fields) => Some(fields),
            Self::Null
            | Self::Bool(_)
            | Self::Int(_)
            | Self::Float(_)
            | Self::Str(_)
            | Self::List(_) => None,
        }
    }
}

/// `str()` of a Python container: the elements' own `str`, comma-space separated.
fn joined(items: &[Value], open: &str, close: &str) -> String {
    let inner: Vec<String> = items.iter().map(Value::py_str).collect();
    format!("{open}{}{close}", inner.join(", "))
}

impl From<f64> for Value {
    fn from(number: f64) -> Self {
        Self::Float(number)
    }
}

impl From<i64> for Value {
    fn from(number: i64) -> Self {
        Self::Int(number)
    }
}

impl From<bool> for Value {
    fn from(flag: bool) -> Self {
        Self::Bool(flag)
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::Str(text)
    }
}

/// An `Option` becomes the value or `null`, which is how every optional reading in a
/// snapshot -- a temperature with no sensor, a VRAM figure Intel does not publish --
/// reaches the JSON.
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

/// A Python `dict` with string keys: ordered by first insertion, and staying there.
///
/// An association list rather than a map because these objects hold at most a dozen
/// keys and the order is the point. Lookup is a scan, which at this size beats hashing
/// and costs nothing next to the file read that produced it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Object(Vec<(String, Value)>);

impl Object {
    /// An empty dict -- also what `load_state` returns for a file it could not read.
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }

    /// `dict[key] = value`. An existing key keeps its position and takes the new
    /// value; a new key goes on the end. This is the whole reason the type exists.
    pub(crate) fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        let key = key.into();
        let value = value.into();
        match self.0.iter_mut().find(|(existing, _)| *existing == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    /// `dict.get(key)`, with a present-but-null key returning the null rather than
    /// nothing -- the distinction `state.get("display", "--")` turns on.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|(existing, _)| existing == key)
            .map(|(_, value)| value)
    }

    /// `dict.setdefault(key, value)`: insert only if the key is absent. A key present
    /// and holding `null` counts as present, exactly as it does in Python.
    pub(crate) fn set_default(&mut self, key: &str, value: Value) {
        if self.get(key).is_none() {
            self.insert(key, value);
        }
    }

    /// `dict.update(other)`, which is [`Object::insert`] for each of `other`'s pairs in
    /// `other`'s own order -- so the new keys land in the merged dict in the order the
    /// sample built them.
    pub(crate) fn update(&mut self, other: Self) {
        for (key, value) in other.0 {
            self.insert(key, value);
        }
    }

    /// Whether there are no keys at all, for [`Value::is_truthy`].
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The pairs in insertion order, for the encoder.
    pub(crate) fn pairs(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }

    /// The values alone, for `str(dict)`'s shape.
    fn values(&self) -> Vec<Value> {
        self.0.iter().map(|(_, value)| value.clone()).collect()
    }
}

/// Build an [`Object`] from a literal list of pairs, so a dict in the Python reads as a
/// dict here instead of half a dozen `insert` calls that hide the key order.
macro_rules! object {
    ($($key:literal => $value:expr),* $(,)?) => {{
        let mut object = $crate::json::Object::new();
        $(object.insert($key, $value);)*
        object
    }};
}

pub(crate) use object;

#[cfg(test)]
mod tests {
    use super::{Object, Value};

    #[test]
    fn assigning_to_an_existing_key_leaves_it_where_it_was() {
        let mut object = Object::new();
        object.insert("history", Value::Int(1));
        object.insert("display", Value::str("42%"));
        object.insert("history", Value::Int(2));

        let keys: Vec<&str> = object.pairs().map(|(key, _)| key).collect();
        assert_eq!(keys, ["history", "display"]);
        assert_eq!(object.get("history"), Some(&Value::Int(2)));
    }

    #[test]
    fn update_appends_new_keys_in_the_others_order() {
        let mut object = Object::new();
        object.insert("history", Value::Int(1));
        let mut other = Object::new();
        other.insert("value", Value::Float(1.5));
        other.insert("history", Value::Int(9));
        other.insert("display", Value::str("x"));
        object.update(other);

        let keys: Vec<&str> = object.pairs().map(|(key, _)| key).collect();
        assert_eq!(keys, ["history", "value", "display"]);
    }

    #[test]
    fn set_default_treats_a_null_as_present() {
        let mut object = Object::new();
        object.insert("history", Value::Null);
        object.set_default("history", Value::Int(0));
        assert_eq!(object.get("history"), Some(&Value::Null));
    }

    #[test]
    fn truthiness_is_pythons_not_presence() {
        assert!(!Value::Null.is_truthy());
        assert!(!Value::Int(0).is_truthy());
        assert!(!Value::Float(0.0).is_truthy());
        assert!(!Value::str("").is_truthy());
        assert!(!Value::List(vec![]).is_truthy());
        assert!(!Value::Object(Object::new()).is_truthy());
        assert!(Value::Float(0.001).is_truthy());
        assert!(Value::str("0").is_truthy());
    }

    #[test]
    fn py_str_spells_the_scalars_the_way_python_does() {
        assert_eq!(Value::Null.py_str(), "None");
        assert_eq!(Value::Bool(true).py_str(), "True");
        assert_eq!(Value::Bool(false).py_str(), "False");
        assert_eq!(Value::Int(7).py_str(), "7");
        assert_eq!(Value::Float(7.0).py_str(), "7.0");
        assert_eq!(Value::Float(0.1 + 0.2).py_str(), "0.30000000000000004");
        assert_eq!(Value::str("n/a").py_str(), "n/a");
    }
}
