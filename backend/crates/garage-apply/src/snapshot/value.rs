//! Two conversions the envelope needs and nothing else in this crate does: a display record
//! and the whole preference document, as `serde_json` values.
//!
//! Kept apart from the readers so each of those stays a statement about one `hyprctl` or
//! `pactl` call. Both are shape-only -- no question is asked of the machine here.

use garage_core::schema::Preferences;
use garage_render::displays::{DisplayEntry, LayoutValue};
use serde_json::{Map, Value};

/// Every display record, as the envelope's `displays` array.
pub(crate) fn entries_value(entries: &[DisplayEntry]) -> Value {
    Value::Array(entries.iter().map(entry_value).collect())
}

/// One display record, keys in the order `display_snapshot()` inserted them.
fn entry_value(entry: &DisplayEntry) -> Value {
    let mut record = Map::new();
    for (key, value) in entry.fields() {
        record.insert(key.clone(), layout_value(value));
    }
    Value::Object(record)
}

/// [`LayoutValue`] as JSON. A TOML datetime cannot appear in a snapshot record -- every field
/// comes from `hyprctl`'s own JSON or from a `displays.toml` value already coerced to a
/// number -- but a hand-edited file could still put one in a field the fold copies through,
/// and `json.dumps` would spell it with `str()`, which is its source text.
fn layout_value(value: &LayoutValue) -> Value {
    match value {
        LayoutValue::Null => Value::Null,
        LayoutValue::Bool(flag) => Value::Bool(*flag),
        LayoutValue::Int(number) => Value::from(*number),
        LayoutValue::Float(number) => Value::from(*number),
        LayoutValue::Str(text) | LayoutValue::Datetime(text) => Value::String(text.clone()),
        LayoutValue::Array(items) => Value::Array(items.iter().map(layout_value).collect()),
        LayoutValue::Table(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), layout_value(item)))
                .collect(),
        ),
    }
}

/// The whole effective configuration, as `response(config)` and `make_snapshot()`'s
/// `preferences` field both print it.
///
/// Sections and keys come out in declaration order, which is `preferences.defaults.toml`'s own
/// order, which is the order the Python's `deep_merge` leaves in the dict it answers with --
/// so a client reading the two backends' envelopes side by side sees the same document, not a
/// re-sorted one.
///
/// `stamp` is `deep_merge(defaults, stored)["schema"]`, which a [`Preferences`] cannot hold --
/// see [`garage_prefs::Effective`]. It is written *first*, which is where the merge leaves it:
/// the shipped defaults file opens with `[schema]`, and `deep_merge` preserves the base's key
/// order. An empty table is omitted entirely, which is the case where neither layer carried a
/// stamp.
///
/// **Parity gap, stated plainly:** the Python's `config` is a dict and can still be carrying
/// keys the schema does not have -- a stamped file's unknown key survives the load. A
/// [`Preferences`] cannot, so those are absent here. Nothing reads them off this envelope: the
/// QML client asks for keys it knows. A second, narrower one: on a machine with *no* shipped
/// defaults file, the Python's base is the compiled `FALLBACK_DEFAULTS`, which has no
/// `[schema]` at all, so a stored stamp would be appended last rather than first. Every
/// stowed machine has the file.
#[must_use]
pub fn preferences_json(config: &Preferences, stamp: &toml::Table) -> Value {
    let mut document = Map::new();
    if !stamp.is_empty() {
        document.insert(
            "schema".to_owned(),
            Value::Object(
                stamp
                    .iter()
                    .map(|(key, value)| (key.clone(), toml_value(value)))
                    .collect(),
            ),
        );
    }
    config.each_key(|key, value| {
        let section = document
            .entry(key.section().as_str().to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(table) = section.as_object_mut() {
            table.insert(key.name().to_owned(), toml_value(&value));
        }
    });
    Value::Object(document)
}

/// A stored TOML scalar as JSON. A TOML datetime cannot reach a [`Preferences`] field -- no
/// kind in the table stores one -- so it is spelled the way TOML spells it rather than given
/// a shape of its own.
fn toml_value(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(text) => Value::String(text.clone()),
        toml::Value::Integer(number) => Value::from(*number),
        toml::Value::Float(number) => Value::from(*number),
        toml::Value::Boolean(flag) => Value::Bool(*flag),
        toml::Value::Datetime(stamp) => Value::String(stamp.to_string()),
        toml::Value::Array(items) => Value::Array(items.iter().map(toml_value).collect()),
        toml::Value::Table(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), toml_value(item)))
                .collect(),
        ),
    }
}
