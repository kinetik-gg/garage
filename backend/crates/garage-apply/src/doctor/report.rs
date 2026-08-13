//! `doctor_report()`: the whole of `garage doctor --report` as one JSON-serializable object.
//!
//! This is the bug-reporting pipeline: one blob a user can paste, carrying the same checks
//! [`crate::doctor::checks`]' `doctor_checks()` walks -- so the printed report and this one
//! cannot drift apart -- plus the three facts a maintainer asks for first (generation
//! timestamp, checkout commit, Hyprland version) and the coercion notes `preferences.toml`
//! produced on the way past.
//!
//! # What is in it, and why none of it is a secret
//!
//! Everything here is either a version string, a path, a systemd unit name or a preference
//! value, and the preference values are the part that genuinely needed checking rather than
//! assuming: the notes carry the offending value verbatim, so the schema was read key by key.
//! Every one of its entries -- the version stamp and every setting across `appearance`,
//! `general`, `input`, `lock`, `region` and `workspaces` -- is a colour, a timeout, an enum
//! choice, a number, a wallpaper path, a locale, a terminal command or a search URL template.
//! There is no password, token, key or credential of any kind in the schema, because Garage
//! has nothing to authenticate to: no account, no sync, no remote. The closest thing to
//! free-form user text is the custom search URL template, and it is a template rather than a
//! secret.
//!
//! Nothing outside layer 2 is read into the report at all -- no environment, no journal, no
//! file contents beyond the preferences themselves. A key that could hold a credential must
//! not be added to the preference schema without revisiting this doc: the notes are printed
//! with the offending value in them, and that is the door a future key could open.
//!
//! # `packages`, and the one place this is wider than the Python
//!
//! The Python reports `package_versions(DOCTOR_PACKAGES)` -- the eight critical names. This
//! reports every package in `packages.list`, because the report exists to be pasted into a
//! bug and "which versions is this machine on" is the question a maintainer asks next. The
//! *check* is still the critical subset, which is what the file's own header says the flag is
//! for. See [`crate::doctor`]'s module doc.
//!
//! Doc-only in one respect: this assembles a value and prints it, and is reached from
//! `doctor --report`'s own dispatch rather than through the response envelope.

use std::fmt::Write as _;

use serde_json::{Map, Value};

use super::checks::{checkout_commit, package_versions};
use super::plugins::{hyprland_report, hyprland_version};
use super::{doctor_checks, local_iso8601, now_seconds, DoctorCx};

/// The report, as one ordered JSON object.
pub(super) fn doctor_report(cx: &DoctorCx<'_>) -> Value {
    let mut checks = Vec::new();
    for (name, probe) in doctor_checks() {
        let verdict = probe(cx);
        let mut entry = Map::new();
        entry.insert("name".to_owned(), Value::String(name.to_owned()));
        entry.insert(
            "status".to_owned(),
            Value::String(verdict.status.word().to_owned()),
        );
        entry.insert("detail".to_owned(), Value::String(verdict.detail));
        entry.insert("hint".to_owned(), Value::String(verdict.hint));
        checks.push(Value::Object(entry));
    }
    let mut notes: Vec<String> = Vec::new();
    if let Err(error) = garage_prefs::load_preferences(cx.paths, Some(&mut notes)) {
        // Already a FAIL in `checks` above, with the same message. Kept here too so the notes
        // list is never silently empty for the one reason that matters -- an unreadable file
        // produces no notes and no settings either.
        notes = vec![format!("could not be loaded: {error}")];
    }
    // Assembled in the Python's own order, which is also the order the probes run in: the
    // dict literal there evaluates `generated_at`, `checkout_commit()`, `hyprland_report()`
    // and `package_versions()` left to right, so a port that computed the packages first
    // would ask the machine the same questions in a different order -- and the trace would
    // say so.
    let mut payload = Map::new();
    payload.insert(
        "generated_at".to_owned(),
        Value::String(local_iso8601(now_seconds())),
    );
    payload.insert(
        "garage_commit".to_owned(),
        Value::String(checkout_commit(cx, &cx.root)),
    );
    payload.insert(
        "hyprland_version".to_owned(),
        Value::String(hyprland_version(&hyprland_report(cx))),
    );
    payload.insert("checks".to_owned(), Value::Array(checks));
    payload.insert(
        "preferences_notes".to_owned(),
        Value::Array(notes.into_iter().map(Value::String).collect()),
    );
    let names: Vec<String> = cx
        .packages()
        .map(|entries| entries.into_iter().map(|entry| entry.name).collect())
        .unwrap_or_default();
    let mut packages = Map::new();
    for (name, version) in package_versions(cx, &names) {
        packages.insert(name, version.map_or(Value::Null, Value::String));
    }
    payload.insert("packages".to_owned(), Value::Object(packages));
    Value::Object(payload)
}

/// Print the report and answer the health question with the exit status.
///
/// Indented, because the whole point is that a person copies it into an issue and someone
/// else reads it there. One JSON object on stdout and nothing beside it -- no banner, no
/// progress line -- which is what makes the output safe to pipe.
pub(super) fn report_text(cx: &DoctorCx<'_>) -> (String, i32) {
    let payload = doctor_report(cx);
    let text = format!("{}\n", dumps_indent2(&payload));
    let failed = payload
        .get("checks")
        .and_then(Value::as_array)
        .is_some_and(|checks| {
            checks.iter().any(|check| {
                check.get("status").and_then(Value::as_str) == Some(super::Status::Fail.word())
            })
        });
    (text, i32::from(failed))
}

/// `json.dumps(value, indent=2)`, `CPython`'s way.
///
/// The same three rules as the envelope's encoder in `garage-cli`, with `indent=2`'s
/// separators instead of the compact ones: keys in insertion order, `ensure_ascii=True` so
/// every code point above `~` becomes `\uXXXX`, and an empty container printed as `{}` / `[]`
/// with no newline inside it.
fn dumps_indent2(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, 0, &mut out);
    out
}

fn write_value(value: &Value, depth: usize, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => {
            let _ = write!(out, "{number}");
        }
        Value::String(text) => write_string(text, out),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(",\n");
                }
                indent(depth + 1, out);
                write_value(item, depth + 1, out);
            }
            out.push('\n');
            indent(depth, out);
            out.push(']');
        }
        Value::Object(map) if map.is_empty() => out.push_str("{}"),
        Value::Object(map) => {
            out.push_str("{\n");
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push_str(",\n");
                }
                indent(depth + 1, out);
                write_string(key, out);
                out.push_str(": ");
                write_value(item, depth + 1, out);
            }
            out.push('\n');
            indent(depth, out);
            out.push('}');
        }
    }
}

fn indent(depth: usize, out: &mut String) {
    for _ in 0..depth * 2 {
        out.push(' ');
    }
}

/// `CPython`'s `py_encode_basestring_ascii()`. A local copy of `garage-cli`'s `pyjson`, which
/// is private to that crate; the shared home for both is `garage-core`, as a follow-up.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for letter in text.chars() {
        match letter {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (' '..='~').contains(&other) => out.push(other),
            other => {
                let code_point = u32::from(other);
                if code_point > 0xffff {
                    let shifted = code_point - 0x1_0000;
                    let _ = write!(
                        out,
                        "\\u{:04x}\\u{:04x}",
                        0xd800 + (shifted >> 10),
                        0xdc00 + (shifted & 0x3ff)
                    );
                } else {
                    let _ = write!(out, "\\u{code_point:04x}");
                }
            }
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::dumps_indent2;

    /// Every expectation is the literal
    /// `python3 -c 'import json; print(json.dumps(..., indent=2))'` output.
    #[test]
    fn the_indent_is_cpythons_indent() {
        assert_eq!(
            dumps_indent2(&json!({"a": 1, "b": [1, 2]})),
            "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}"
        );
        assert_eq!(
            dumps_indent2(&json!({"a": [], "b": {}})),
            "{\n  \"a\": [],\n  \"b\": {}\n}"
        );
        assert_eq!(dumps_indent2(&json!(null)), "null");
    }

    #[test]
    fn ensure_ascii_is_on_here_too() {
        assert_eq!(
            dumps_indent2(&json!(["caf\u{e9}"])),
            "[\n  \"caf\\u00e9\"\n]"
        );
    }

    /// Insertion order, which is what makes the report's first three fields readable.
    #[test]
    fn keys_keep_the_order_they_were_inserted_in() {
        let mut map = serde_json::Map::new();
        map.insert("zebra".to_owned(), json!(1));
        map.insert("apple".to_owned(), json!(2));
        assert_eq!(
            dumps_indent2(&serde_json::Value::Object(map)),
            "{\n  \"zebra\": 1,\n  \"apple\": 2\n}"
        );
    }
}
