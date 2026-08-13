//! Layer 2 as a document: the departures from layer 1, and where a note about the file goes.
//!
//! `preference_sections()`, `same_default()`, `preference_deltas()`,
//! `report_preference_notes()` and `preference_document()`, in the Python's own order. Two
//! callers reach this module and they hand it different things: `compact_preferences_file()`
//! passes the *raw stored table*, unvalidated, straight off the disk, and
//! `save_preferences()` passes an effective configuration. That is why everything here works
//! on [`toml::Table`] rather than on [`Preferences`] -- the compaction's whole job is to
//! notice keys the schema does not have, and a typed value cannot hold one.

use garage_core::schema::prefs::Preferences;
use garage_core::toml_emit::{dump_toml, Document, EmitError, Section, Value};

use crate::migrate::PREFERENCES_VERSION;
use crate::pyvalue::py_equal;

/// A preferences document without its `[schema]` stamp.
///
/// The stamp is bookkeeping, not a preference: it is written unconditionally and compared
/// separately, so every comparison of "what does this file actually say" leaves it out.
#[must_use]
pub fn preference_sections(document: &toml::Table) -> toml::Table {
    document
        .iter()
        .filter(|(name, _)| name.as_str() != "schema")
        .map(|(name, values)| (name.clone(), values.clone()))
        .collect()
}

/// Whether a stored value is the shipped default rather than a departure.
///
/// `==` alone would be wrong in one direction and right in the other, so the two cases are
/// split:
///
///   * bool is a subclass of int in Python, so `True == 1` and `False == 0`. A stored `true`
///     against a default of `1` is a different kind of value, not the same one, and dropping
///     it would hand the renderers an int where the file said bool. So the bool-ness has to
///     match. (Rust would not confuse the two on its own; the check is here because the
///     *other* half deliberately restores Python's numeric equality, and a bool has to stay
///     outside it.)
///   * `1 == 1.0`, and there the numbers really are the same value. The schema ships
///     `pointer_sensitivity` as `0.0` and a UI that sends JSON `0` stores an int; treating
///     that as a departure would pin a copy of the default in layer 2 forever over nothing
///     but a decimal point. Layer 1 owns the type as well as the value, so the int is
///     dropped and the float comes back.
#[must_use]
pub fn same_default(value: &toml::Value, default: &toml::Value) -> bool {
    if value.as_bool().is_some() != default.as_bool().is_some() {
        return false;
    }
    py_equal(value, default)
}

/// The parts of `stored` that depart from `defaults`, and nothing else.
///
/// Recursive, and driven by `defaults` rather than by `stored`, which gives two things at
/// once: the result comes out in the shipped file's own key order, and a key the schema does
/// not have cannot reach the result at all. Those keys are appended to `dropped` instead --
/// a withdrawn setting, a typo in a hand edit, or a downgrade to a build that no longer has
/// it. The coercion pass already ignores them, so all they do is sit in the file looking
/// meaningful.
#[must_use]
pub fn preference_deltas(
    stored: &toml::Table,
    defaults: &toml::Table,
    mut dropped: Option<&mut Vec<String>>,
    prefix: &str,
) -> toml::Table {
    let mut deltas = toml::Table::new();
    for (key, default) in defaults {
        let Some(value) = stored.get(key) else {
            continue;
        };
        if let Some(delta) = one_delta(key, value, default, dropped.as_deref_mut(), prefix) {
            deltas.insert(key.clone(), delta);
        }
    }
    if let Some(dropped) = dropped {
        let unknown = stored.keys().filter(|key| !defaults.contains_key(*key));
        dropped.extend(unknown.map(|key| format!("{prefix}{key}")));
    }
    deltas
}

/// One key's contribution: the value itself when it departs, a nested table when the key is
/// a section with departures under it, and `None` when it is neither.
fn one_delta(
    key: &str,
    value: &toml::Value,
    default: &toml::Value,
    dropped: Option<&mut Vec<String>>,
    prefix: &str,
) -> Option<toml::Value> {
    if !default.is_table() && !value.is_table() {
        return if same_default(value, default) {
            None
        } else {
            Some(value.clone())
        };
    }
    let nested = nested_deltas(value, default, dropped, prefix, key)?;
    if nested.is_empty() {
        None
    } else {
        Some(toml::Value::Table(nested))
    }
}

/// One step down, when either side is a table.
///
/// `None` means the two sides disagree about their shape -- a section where a value belongs,
/// or the reverse. Nothing can be done with it and merging it would corrupt the shape, so it
/// joins `dropped` and the caller emits nothing for the key.
fn nested_deltas(
    value: &toml::Value,
    default: &toml::Value,
    dropped: Option<&mut Vec<String>>,
    prefix: &str,
    key: &str,
) -> Option<toml::Table> {
    let (Some(value), Some(default)) = (value.as_table(), default.as_table()) else {
        if let Some(dropped) = dropped {
            dropped.push(format!("{prefix}{key}"));
        }
        return None;
    };
    Some(preference_deltas(
        value,
        default,
        dropped,
        &format!("{prefix}{key}."),
    ))
}

/// Where a note about the stored file goes.
///
/// stderr, which is the journal under the units that render at session start, or `sink` when
/// a caller needs to count the notes instead -- `garage doctor` has to report them, and
/// reading its own stderr back is not something a process can do honestly.
///
/// The sink is threaded through the whole load and save chain rather than collected and
/// returned, because the Python prints at the moment the note is produced: a save that
/// reports a dropped key and *then* fails to write has already said so, and a port that only
/// handed its notes back on the success path would swallow that line.
pub fn report_preference_notes(notes: &[String], sink: Option<&mut Vec<String>>) {
    match sink {
        None => {
            for note in notes {
                eprintln!("garage: preferences.toml: {note}");
            }
        }
        Some(sink) => sink.extend(notes.iter().cloned()),
    }
}

/// Layer 2 as a document: the version stamp, then the departures from layer 1.
///
/// Nothing else. The whole merged configuration used to be written here, and that is what
/// put a frozen copy of all ~50 shipped defaults into the first file a fresh install ever
/// wrote. From then on the shipped defaults were dead to that machine: every one of them was
/// overridden by a copy of its own old value, so changing a default in a release could never
/// reach an install that had ever touched a setting, and a key removed from the schema
/// stayed in the file forever.
///
/// The consequence to be deliberate about: a key whose value equals the shipped default is
/// *absent* from the file, so setting a value back to the default erases the delta rather
/// than pinning it. That is the intended meaning -- "follow the shipped default again" --
/// under a schema whose defaults are expected to move. There is deliberately no way to pin
/// today's default against a future change to it: layer 2 records what the user chose
/// *differently*, and "the same as what shipped" is not a choice that outlives the shipping
/// changing.
///
/// The stamp is written rather than diffed. It equals the default by construction, so a diff
/// would drop it, and a file with no `[schema]` section reads as version 1 -- which would
/// replay every migration over it on the next load. Nothing else in `[schema]` is carried:
/// it is this program's bookkeeping, not a place to keep settings. A document that is
/// nothing but the stamp is therefore the file of a machine sitting on factory state, and
/// writing one is how a departure is taken back.
#[must_use]
pub fn preference_document(
    config: &toml::Table,
    defaults: &Preferences,
    sink: Option<&mut Vec<String>>,
) -> toml::Table {
    let mut dropped: Vec<String> = Vec::new();
    let mut document = toml::Table::new();
    let mut schema = toml::Table::new();
    schema.insert(
        "preferences_version".to_owned(),
        toml::Value::Integer(PREFERENCES_VERSION),
    );
    document.insert("schema".to_owned(), toml::Value::Table(schema));
    let deltas = preference_deltas(
        &preference_sections(config),
        &preference_sections(&preferences_table(defaults)),
        Some(&mut dropped),
        "",
    );
    document.extend(deltas);
    let notes: Vec<String> = dropped
        .iter()
        .map(|name| format!("{name} is not a preference this build has; dropping it"))
        .collect();
    report_preference_notes(&notes, sink);
    document
}

/// A typed configuration in the file's own shape: nested section, then key.
///
/// The bridge between the schema's types and this module, which works on tables because the
/// compaction has to see keys no type can hold. Sections and keys come out in declaration
/// order, which `declared_order_matches_the_file` pins to `preferences.defaults.toml`'s own
/// order -- so a document built from this is emitted in the shipped file's order, exactly as
/// the Python's defaults-driven walk produces.
#[must_use]
pub fn preferences_table(preferences: &Preferences) -> toml::Table {
    let mut table = toml::Table::new();
    preferences.each_key(|key, value| {
        let section = table
            .entry(key.section().as_str().to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let Some(section) = section.as_table_mut() {
            section.insert(key.name().to_owned(), value);
        }
    });
    table
}

/// A document as `dump_toml()` writes one.
///
/// The Python hands its dict straight to `dump_toml()`; here the emitter has a type of its
/// own ([`Document`]), pinned byte-for-byte against the Python in task 2.5, so this is the
/// conversion into it. Two of the Python's skips are performed here rather than there, and
/// both are invisible in the output: a top-level value that is not a section is dropped
/// (`if not isinstance(values, dict): continue`) and so is a key holding an array or a table
/// (`if not isinstance(value, (dict, list))`). Doing the second one early is what keeps a
/// datetime *inside* a skipped container from being refused -- the Python never looks at it
/// either.
///
/// # Errors
///
/// [`EmitError`] for a non-finite float, and for a TOML date, time or datetime sitting where
/// a scalar belongs: `toml_value()` has no branch for one, so the Python raises
/// `f"Unsupported TOML value: {value!r}"` and the payload here is that same `repr`.
pub fn emit_document(document: &toml::Table) -> Result<String, EmitError> {
    let mut out = Document::new();
    for (name, values) in document {
        let Some(values) = values.as_table() else {
            continue;
        };
        let mut section = Section::new();
        for (key, value) in values {
            if let Some(value) = emit_value(value)? {
                section.push(key.clone(), value);
            }
        }
        out.push(name.clone(), section);
    }
    dump_toml(&out)
}

/// One stored value as the emitter's own. `None` is a container, which `dump_toml()` skips.
fn emit_value(value: &toml::Value) -> Result<Option<Value>, EmitError> {
    Ok(Some(match value {
        toml::Value::Boolean(flag) => Value::Bool(*flag),
        toml::Value::Integer(number) => Value::Int(*number),
        toml::Value::Float(number) => Value::Float(*number),
        toml::Value::String(text) => Value::Str(text.clone()),
        toml::Value::Array(_) | toml::Value::Table(_) => return Ok(None),
        toml::Value::Datetime(_) => {
            return Err(EmitError::Unsupported(
                garage_core::schema::notes::py_repr_toml(value),
            ))
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        emit_document, preference_deltas, preference_document, preference_sections,
        report_preference_notes, same_default,
    };
    use garage_core::schema::Defaults;

    fn table(text: &str) -> toml::Table {
        text.parse().unwrap()
    }

    fn value(text: &str) -> toml::Value {
        table(&format!("value = {text}")).remove("value").unwrap()
    }

    #[test]
    fn the_stamp_is_not_a_section() {
        let document = table("[schema]\npreferences_version = 5\n[appearance]\nborder_size = 1\n");
        let sections = preference_sections(&document);
        assert!(!sections.contains_key("schema"));
        assert_eq!(sections.len(), 1);
    }

    #[test]
    fn an_int_is_the_float_it_equals_but_a_bool_is_not_the_int() {
        assert!(same_default(&value("0"), &value("0.0")));
        assert!(same_default(&value("1"), &value("1.0")));
        assert!(!same_default(&value("true"), &value("1")));
        assert!(!same_default(&value("1"), &value("true")));
        assert!(same_default(&value("true"), &value("true")));
        assert!(!same_default(&value("2"), &value("1.0")));
        assert!(!same_default(&value("false"), &value("0")));
        assert!(!same_default(&value("0"), &value("false")));
    }

    #[test]
    fn a_key_the_schema_does_not_have_is_dropped_rather_than_kept() {
        let defaults = table("[appearance]\naccent_color = \"blue\"\n");
        let stored = table("[appearance]\naccent_color = \"red\"\nnot_a_key = 1\n[bogus]\nx = 1\n");
        let mut dropped = Vec::new();
        let deltas = preference_deltas(&stored, &defaults, Some(&mut dropped), "");
        assert_eq!(
            deltas,
            table("[appearance]\naccent_color = \"red\"\n"),
            "only the departure survives"
        );
        assert_eq!(dropped, ["appearance.not_a_key", "bogus"]);
    }

    /// A section where a value belongs, and the reverse: neither can be merged, so both
    /// are named as dropped and nothing is emitted for them.
    #[test]
    fn a_shape_mismatch_is_dropped_from_either_side() {
        let defaults = table("[appearance]\naccent_color = \"blue\"\n");
        let mut dropped = Vec::new();
        let deltas = preference_deltas(
            &table("appearance = \"hi\"\n"),
            &defaults,
            Some(&mut dropped),
            "",
        );
        assert!(deltas.is_empty());
        assert_eq!(dropped, ["appearance"]);

        let mut dropped = Vec::new();
        let deltas = preference_deltas(
            &table("[appearance]\n[appearance.accent_color]\nx = 1\n"),
            &defaults,
            Some(&mut dropped),
            "",
        );
        assert!(deltas.is_empty());
        assert_eq!(dropped, ["appearance.accent_color"]);
    }

    #[test]
    fn an_empty_document_is_the_stamp_alone() {
        let defaults = Defaults::compiled().unwrap();
        let mut sink = Vec::new();
        let document = preference_document(&toml::Table::new(), defaults.values(), Some(&mut sink));
        assert_eq!(
            emit_document(&document).unwrap(),
            "[schema]\npreferences_version = 5\n"
        );
        assert!(sink.is_empty());
    }

    #[test]
    fn a_departure_appears_and_a_default_does_not() {
        let defaults = Defaults::compiled().unwrap();
        let stored = table("[appearance]\naccent_color = \"red\"\ncorner_radius = \"normal\"\n");
        let document = preference_document(&stored, defaults.values(), None);
        assert_eq!(
            emit_document(&document).unwrap(),
            "[schema]\npreferences_version = 5\n\n[appearance]\naccent_color = \"red\"\n"
        );
    }

    #[test]
    fn the_dropped_note_is_the_python_f_string() {
        let defaults = Defaults::compiled().unwrap();
        let stored = table("[appearance]\nnot_a_key = 1\n");
        let mut sink = Vec::new();
        drop(preference_document(
            &stored,
            defaults.values(),
            Some(&mut sink),
        ));
        assert_eq!(
            sink,
            ["appearance.not_a_key is not a preference this build has; dropping it"]
        );
    }

    #[test]
    fn a_sink_collects_what_stderr_would_have_carried() {
        let mut sink = Vec::new();
        report_preference_notes(&["one".to_owned()], Some(&mut sink));
        report_preference_notes(&["two".to_owned()], Some(&mut sink));
        assert_eq!(sink, ["one", "two"]);
    }

    /// `dump_toml()` skips a container, so a datetime inside one is never looked at --
    /// but a datetime standing where a scalar belongs is refused, with the `repr` the
    /// Python's f-string produces.
    #[test]
    fn a_datetime_is_refused_the_way_toml_value_refuses_one() {
        let error = emit_document(&table("[appearance]\ntheme_light_at = 07:00:00\n")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Unsupported TOML value: datetime.time(7, 0)"
        );
        let error =
            emit_document(&table("[appearance]\ntheme_light_at = 1979-05-27\n")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Unsupported TOML value: datetime.date(1979, 5, 27)"
        );
        assert_eq!(
            emit_document(&table("[appearance]\nlist = [07:00:00]\nkept = 1\n")).unwrap(),
            "[appearance]\nkept = 1\n"
        );
    }

    #[test]
    fn a_non_finite_float_is_refused_with_the_python_message() {
        let error = emit_document(&table("[appearance]\nanimation_speed = nan\n")).unwrap_err();
        assert_eq!(error.to_string(), "Non-finite numbers are not supported");
    }

    #[test]
    fn a_top_level_value_that_is_not_a_section_is_skipped() {
        assert_eq!(emit_document(&table("loose = 1\n")).unwrap(), "\n");
    }
}
