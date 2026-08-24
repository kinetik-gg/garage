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

    /// The value as a borrowed object, when it is one.
    pub(crate) fn as_object(&self) -> Option<&Object> {
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

    /// The pairs in insertion order, for the encoder.
    pub(crate) fn pairs(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
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
}
