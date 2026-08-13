//! Byte-parity tests for the whole keybind layer against the real Python backend.
//!
//! `testdata/keybind_fixtures.json` is the output of a throwaway script (not committed -- see
//! this task's report) that loads `desktop/.local/bin/garage` with `SourceFileLoader`, the
//! same way `tests/harness.py` does, and records what the Python produced for every case
//! below. Six families:
//!
//! * `parse` -- one entry per combination string, carrying `combination_id`,
//!   `canonical_combination`, `combination_signature` and `require_bindable`, each as the
//!   value or as the `SettingsError` text.
//! * `key_choices` -- the whole key whitelist, in order.
//! * `catalogs` -- a published catalog's bytes against the entries and the *verified* flag
//!   they read back as. This is the witness-line contract's own table: well-formed, missing
//!   witness, wrong count, short count, torn line, torn line with a stale witness, empty,
//!   witness-only, duplicate ids, four fields, six fields, empty id, blank lines after the
//!   witness, a witness with a trailing space, no trailing newline, and a protected flag
//!   that is not `1`.
//! * `documents` -- every `keybindings.toml` shape against three catalog states, carrying the
//!   loaded document, the stderr it printed, `keybindings_toml`, `keybinds.json`,
//!   `resolve_keybinds` and the guard's verdict.
//! * `guards` -- the refusals on their own, including the two that need a document the loader
//!   would have filtered.
//! * `actions` -- one `keybind_action()` call per entry, with the files before and after and
//!   the refusal text where it refused.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use garage_core::paths::Paths;
use garage_core::traits::{LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource};
use garage_render::keybinds::{keybinds_json, CustomKeybind, Document, Overrides};
use serde_json::Value;

use crate::keybind::catalog::{read_keybind_catalog, resolve_keybinds};
use crate::keybind::guard::guard_keybinds;
use crate::keybind::parse::{
    canonical_combination, combination_id, combination_signature, key_choices, require_bindable,
};
use crate::keybind::store::{keybindings_toml, load_keybindings};
use crate::keybind::KeybindError;

const FIXTURES: &str = include_str!("../../testdata/keybind_fixtures.json");

static SERIAL: AtomicU64 = AtomicU64::new(0);

/// A `HOME` of its own per case, so two tests never share a catalog or a lock file.
pub(crate) fn scratch_paths(label: &str) -> Paths {
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let home = std::env::temp_dir().join(format!(
        "garage-keybind-{label}-{}-{serial}",
        std::process::id()
    ));
    let env: HashMap<String, String> = [("HOME".to_owned(), home.to_string_lossy().into_owned())]
        .into_iter()
        .collect();
    Paths::from_env_map(&env)
}

pub(super) struct NoMonitors;
impl MonitorSource for NoMonitors {
    fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
        Ok(Vec::new())
    }
}

pub(super) struct LuaAccepts;
impl LuaSyntaxCheck for LuaAccepts {
    fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
        Ok(())
    }
}

pub(super) fn fixtures() -> Value {
    serde_json::from_str(FIXTURES).expect("testdata/keybind_fixtures.json is valid JSON")
}

pub(super) fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("fixture is missing {key:?}"))
}

pub(super) fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    field(value, key)
        .as_str()
        .unwrap_or_else(|| panic!("fixture field {key:?} is not a string"))
}

/// Compare one `{"ok": ...}` / `{"error": "..."}` pair against what the Rust side did.
pub(super) fn same_outcome<T: PartialEq + std::fmt::Debug>(
    expected: &Value,
    actual: Result<T, KeybindError>,
    wanted: impl FnOnce(&Value) -> T,
    label: &str,
) {
    match (expected.get("ok"), expected.get("error")) {
        (Some(value), _) => match actual {
            Ok(held) => assert_eq!(held, wanted(value), "{label}"),
            Err(error) => panic!("{label}: refused with {error}, expected {value}"),
        },
        (None, Some(message)) => match actual {
            Ok(held) => panic!("{label}: produced {held:?}, expected refusal {message}"),
            Err(error) => assert_eq!(
                error.to_string(),
                message.as_str().unwrap_or_default(),
                "{label}"
            ),
        },
        (None, None) => panic!("{label}: fixture carries neither ok nor error"),
    }
}

fn as_string(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

#[test]
fn the_key_whitelist_matches_the_python_one() {
    let all = fixtures();
    let expected: Vec<String> = field(&all, "key_choices")
        .as_array()
        .expect("key_choices is an array")
        .iter()
        .map(as_string)
        .collect();
    assert_eq!(key_choices(), expected);
}

#[test]
fn every_combination_parses_the_way_the_python_parses_it() {
    let all = fixtures();
    let cases = field(&all, "parse").as_array().expect("parse is an array");
    assert!(cases.len() >= 45, "the parse table should not have shrunk");
    for case in cases {
        let value = text(case, "value");
        assert_eq!(combination_id(value), text(case, "id"), "id of {value:?}");
        same_outcome(
            field(case, "canonical"),
            canonical_combination(Some(value)),
            as_string,
            &format!("canonical_combination({value:?})"),
        );
        same_outcome(
            field(case, "bindable"),
            require_bindable(Some(value)),
            as_string,
            &format!("require_bindable({value:?})"),
        );
        same_outcome(
            field(case, "signature"),
            combination_signature(value),
            |wanted| {
                let pair = wanted.as_array().expect("a signature is a pair");
                let modifiers = pair
                    .first()
                    .and_then(Value::as_array)
                    .expect("modifiers")
                    .iter()
                    .map(as_string)
                    .collect();
                (modifiers, as_string(pair.get(1).expect("key")))
            },
            &format!("combination_signature({value:?})"),
        );
    }
}

#[test]
fn the_witness_line_contract_reads_every_catalog_the_way_the_python_does() {
    let all = fixtures();
    let cases = field(&all, "catalogs")
        .as_object()
        .expect("catalogs is an object");
    assert!(
        cases.len() >= 16,
        "the catalog table should not have shrunk"
    );
    for (name, case) in cases {
        let paths = scratch_paths(&format!("catalog-{name}"));
        if let Some(written) = field(case, "text").as_str() {
            let target = &paths.fragments.keybinds_catalog;
            std::fs::create_dir_all(target.parent().expect("a parent")).expect("scratch");
            std::fs::write(target, written).expect("the catalog is writable");
        }
        let (catalog, verified) = read_keybind_catalog(&paths);
        assert_eq!(
            verified,
            field(case, "verified").as_bool().expect("a bool"),
            "{name}: verified"
        );
        let entries = field(case, "entries")
            .as_array()
            .expect("entries is an array");
        assert_eq!(catalog.len(), entries.len(), "{name}: row count");
        for (row, expected) in catalog.iter().zip(entries) {
            assert_eq!(row.id, text(expected, "id"), "{name}: id");
            assert_eq!(row.group, text(expected, "group"), "{name}: group");
            assert_eq!(row.keys, text(expected, "keys"), "{name}: keys");
            assert_eq!(
                row.protected,
                field(expected, "protected").as_bool().expect("a bool"),
                "{name}: protected"
            );
            assert_eq!(
                row.description,
                text(expected, "description"),
                "{name}: description"
            );
        }
        drop(std::fs::remove_dir_all(&paths.home));
    }
}

/// Lay a catalog and a `keybindings.toml` down where the loader looks.
pub(super) fn lay_out(paths: &Paths, catalog: &str, keybindings: &str) {
    let target = &paths.fragments.keybinds_catalog;
    std::fs::create_dir_all(target.parent().expect("a parent")).expect("scratch");
    std::fs::write(target, catalog).expect("the catalog is writable");
    let stored = &paths.host.keybindings;
    std::fs::create_dir_all(stored.parent().expect("a parent")).expect("scratch");
    std::fs::write(stored, keybindings).expect("keybindings.toml is writable");
}

fn document_from(case: &Value) -> Document {
    let mut overrides = Overrides::new();
    for pair in field(case, "overrides")
        .as_array()
        .expect("overrides is an array of pairs")
    {
        let pair = pair.as_array().expect("a pair");
        overrides.set(
            &as_string(pair.first().expect("an id")),
            &as_string(pair.get(1).expect("a combination")),
        );
    }
    let custom = field(case, "custom")
        .as_array()
        .expect("custom is an array")
        .iter()
        .map(|item| CustomKeybind {
            id: text(item, "id").to_owned(),
            keys: text(item, "keys").to_owned(),
            description: text(item, "description").to_owned(),
            command: text(item, "command").to_owned(),
        })
        .collect();
    Document { overrides, custom }
}

#[test]
fn every_stored_document_loads_and_writes_back_the_way_the_python_does() {
    let all = fixtures();
    let cases = field(&all, "documents")
        .as_object()
        .expect("documents is an object");
    assert!(
        cases.len() >= 45,
        "the document table should not have shrunk"
    );
    for (name, case) in cases {
        let published = field(&all, "catalogs")
            .get(text(case, "catalog"))
            .map(|catalog| text(catalog, "text").to_owned())
            .unwrap_or_default();
        one_document(name, case, &published);
    }
}

/// One stored `keybindings.toml`, against the catalog state it was recorded under.
fn one_document(name: &str, case: &Value, published: &str) {
    let paths = scratch_paths("document");
    let stored = text(case, "toml");
    lay_out(&paths, published, stored);
    let mut notes = Vec::new();
    let loaded = load_keybindings(&paths, Some(&mut notes));
    let expected = field(case, "document");
    let printed = notes.iter().fold(String::new(), |mut out, note| {
        let _ = writeln!(out, "garage: keybindings.toml: {note}");
        out
    });
    assert_eq!(printed, text(case, "stderr"), "{name}: stderr");
    assert_eq!(
        mask_document(&loaded, stored),
        mask_document(&document_from(expected), stored),
        "{name}: document"
    );
    assert_eq!(
        mask_ids(
            &keybindings_toml(&loaded).expect("a document of strings emits"),
            stored
        ),
        mask_ids(text(expected, "keybindings_toml"), stored),
        "{name}: keybindings.toml"
    );
    assert_eq!(
        keybinds_json(&loaded),
        text(expected, "keybinds_json"),
        "{name}: keybinds.json"
    );
    let (catalog, verified) = read_keybind_catalog(&paths);
    same_resolution(
        &resolve_keybinds(&catalog, &loaded),
        field(case, "resolved"),
        stored,
        name,
    );
    same_outcome(
        field(case, "guard"),
        guard_keybinds(&catalog, &loaded, verified).map(|()| Value::Null),
        |_| Value::Null,
        &format!("{name}: guard"),
    );
    drop(std::fs::remove_dir_all(&paths.home));
}

fn same_resolution(
    resolved: &[crate::keybind::catalog::ResolvedBind],
    expected: &Value,
    source: &str,
    name: &str,
) {
    let expected = expected.as_array().expect("resolved is an array");
    assert_eq!(resolved.len(), expected.len(), "{name}: resolved count");
    for (bind, wanted) in resolved.iter().zip(expected) {
        let held = text(wanted, "id");
        // A custom shortcut whose id neither backend could predict -- see `mask_ids`.
        if !(is_invented(&bind.id, source) && is_invented(held, source)) {
            assert_eq!(bind.id, held, "{name}: resolved id");
        }
        assert_eq!(bind.group, text(wanted, "group"), "{name}: resolved group");
        assert_eq!(bind.keys, text(wanted, "keys"), "{name}: resolved keys");
        assert_eq!(
            bind.default_keys,
            text(wanted, "defaultKeys"),
            "{name}: resolved defaultKeys"
        );
        assert_eq!(
            bind.protected,
            field(wanted, "protected").as_bool().expect("a bool"),
            "{name}: resolved protected"
        );
        assert_eq!(
            bind.description,
            text(wanted, "description"),
            "{name}: resolved description"
        );
        assert_eq!(
            bind.modified,
            field(wanted, "modified").as_bool().expect("a bool"),
            "{name}: resolved modified"
        );
        assert_eq!(
            bind.custom,
            field(wanted, "custom").as_bool().expect("a bool"),
            "{name}: resolved custom"
        );
    }
}

#[test]
fn every_guard_refusal_is_the_pythons_refusal() {
    let all = fixtures();
    let cases = field(&all, "guards")
        .as_object()
        .expect("guards is an object");
    assert!(cases.len() >= 8, "the guard table should not have shrunk");
    for (name, case) in cases {
        let paths = scratch_paths("guard");
        lay_out(&paths, text(case, "catalog"), "");
        let (catalog, _) = read_keybind_catalog(&paths);
        let document = document_from(case);
        let verified = field(case, "verified").as_bool().expect("a bool");
        same_outcome(
            field(case, "guard"),
            guard_keybinds(&catalog, &document, verified).map(|()| Value::Null),
            |_| Value::Null,
            name,
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }
}

/// Replace every invented id with a fixed stand-in.
///
/// `custom_keybind()` invents a twelve-hex-character id when the payload carries none, and
/// neither backend can produce the other's. An id the stored file already carried is *not*
/// masked -- it is the user's, both sides must keep it, and only the invented ones are
/// unknowable. Everything else about the line, the quoting and the position included, is
/// compared as written.
pub(super) fn mask_ids(text: &str, source: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("id = \"") {
            let id = rest.trim_end().trim_end_matches('"');
            if is_invented(id, source) {
                out.push_str("id = \"<invented>\"\n");
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

/// Whether an id is one a backend invented rather than one the stored file carried.
pub(super) fn is_invented(id: &str, source: &str) -> bool {
    id.len() == 12 && id.chars().all(|ch| ch.is_ascii_hexdigit()) && !source.contains(id)
}

/// The document with every invented custom id replaced, for the same reason [`mask_ids`]
/// exists.
fn mask_document(document: &Document, source: &str) -> Document {
    let mut masked = document.clone();
    for item in &mut masked.custom {
        if is_invented(&item.id, source) {
            item.id = "<invented>".to_owned();
        }
    }
    masked
}
