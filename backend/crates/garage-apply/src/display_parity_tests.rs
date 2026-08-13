//! Byte-parity tests for [`crate::displays::config`]: `normalize_display_layout()` and
//! `layout_toml()`, against the real Python backend.
//!
//! `testdata/display_fixtures.json` is the output of a throwaway generator (not committed --
//! see this task's report) that loads `desktop/.local/bin/garage` through `tests/harness.py`,
//! hands each layout to `normalize_display_layout()` and then to `layout_toml()`, and records
//! the normalized value, the emitted text, and the refusal when there was one.
//!
//! The matrix, 25 layouts: an empty layout and one with no `displays` key; one display at the
//! origin and one off it; two shifted into negative space; a disabled display and a mirror,
//! neither of which may anchor the arrangement, and a mirror that is still *shifted* along
//! with it; a mirror whose source is off, which makes it a placed display again and moves the
//! corner; a layout that is all disabled and one that is all mirrors, both of which normalize
//! to nothing; half-pixel coordinates that must round to even rather than away from zero;
//! coordinates given as strings, absent entirely, non-numeric, and null. Then
//! `layout_toml()`'s own shapes: every written key present; a mirror written last; an empty
//! mirror left out; the keys the emitter never writes (`modes`, `focused`, `primary`); an
//! empty primary and one needing JSON quoting; a float scale; and the two values the emitter
//! refuses -- a null and a list.

use garage_render::displays::{DisplayEntry, DisplayLayout, LayoutValue};
use serde_json::Value;

use crate::displays::config::{layout_toml, normalize_display_layout};
use crate::displays::wire::{layout_from_json, to_json};

const FIXTURES: &str = include_str!("../testdata/display_fixtures.json");

fn text(value: &Value, key: &str) -> Option<String> {
    match value.get(key) {
        None | Some(Value::Null) => None,
        Some(found) => found.as_str().map(str::to_owned),
    }
}

/// The Python records `f"{type(error).__name__}: {error}"`.
fn without_exception_name(recorded: &str) -> &str {
    recorded.split_once(": ").map_or(recorded, |(_, rest)| rest)
}

/// One normalized layout as `{"displays": [...], "primary": "..."}`, so a fixture recorded by
/// `json.dumps` can be compared against it without a second value model in the test.
fn as_json(layout: &DisplayLayout) -> Value {
    let displays: Vec<Value> = layout
        .displays
        .iter()
        .map(|entry| {
            let mut out = String::new();
            to_json(&LayoutValue::Table(entry.fields().to_vec()), &mut out);
            serde_json::from_str(&out).expect("a record round-trips through JSON")
        })
        .collect();
    serde_json::json!({ "displays": displays, "primary": layout.primary })
}

/// The Python's `normalize_display_layout()` returns the *whole* deep copy, including any
/// top-level key the port's two-field layout does not model -- and `{}` for a layout that had
/// no `primary` at all, where the port fills in `""`. Both are compared on the two fields the
/// Python's own consumers read.
fn comparable(recorded: &Value) -> Value {
    serde_json::json!({
        "displays": recorded.get("displays").cloned().unwrap_or(Value::Array(Vec::new())),
        "primary": recorded
            .get("primary")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    })
}

#[test]
fn every_layout_normalizes_and_serialises_as_the_python_does() {
    let all: Value =
        serde_json::from_str(FIXTURES).expect("testdata/display_fixtures.json is valid JSON");
    let scenarios = all.as_object().expect("fixture root is an object");
    assert!(scenarios.len() >= 20, "the matrix should not have shrunk");

    for (name, scenario) in scenarios {
        let payload = scenario
            .get("payload")
            .expect("fixture carries its payload");
        let layout = layout_from_json(payload);
        let wanted = text(scenario, "error").unwrap_or_default();

        let outcome = normalize_display_layout(&layout)
            .and_then(|normalized| Ok((as_json(&normalized), layout_toml(&normalized)?)));
        match (outcome, wanted.is_empty()) {
            (Ok((normalized, emitted)), true) => {
                assert_eq!(
                    normalized,
                    comparable(
                        scenario
                            .get("normalized")
                            .expect("fixture carries the normalized layout")
                    ),
                    "{name}: normalize_display_layout"
                );
                assert_eq!(
                    Some(emitted),
                    text(scenario, "layout_toml"),
                    "{name}: layout_toml"
                );
            }
            (Err(error), false) => assert_eq!(
                error.to_string(),
                without_exception_name(&wanted),
                "{name}: refusal"
            ),
            (Ok(_), false) => panic!("{name}: the Python refused this and the port did not"),
            (Err(error), true) => {
                panic!("{name}: the port refused and the Python did not: {error}")
            }
        }
    }
}

/// The rule the coordinate scenarios are cases of: after normalisation the placed displays'
/// top-left corner is exactly the origin, and every other display has moved by the same
/// amount rather than been left where it was.
#[test]
fn a_normalized_layout_is_anchored_at_the_origin() {
    let all: Value =
        serde_json::from_str(FIXTURES).expect("testdata/display_fixtures.json is valid JSON");
    let mut checked = 0;
    for (name, scenario) in all.as_object().expect("fixture root is an object") {
        if !text(scenario, "error").unwrap_or_default().is_empty() {
            continue;
        }
        let layout = layout_from_json(scenario.get("payload").expect("a payload"));
        let normalized = normalize_display_layout(&layout).expect("this one does not refuse");
        let mirrors = garage_render::displays::mirror_targets(&normalized.displays);
        let placed: Vec<&DisplayEntry> = normalized
            .displays
            .iter()
            .filter(|entry| {
                entry.enabled() && !mirrors.iter().any(|(output, _)| *output == entry.output())
            })
            .collect();
        if placed.is_empty() {
            continue;
        }
        let corner = |key: &str| {
            placed
                .iter()
                .filter_map(|entry| entry.get(key).and_then(|held| held.py_int().ok()))
                .min()
        };
        assert_eq!(corner("x"), Some(0), "{name}: the left edge is not at zero");
        assert_eq!(corner("y"), Some(0), "{name}: the top edge is not at zero");
        checked += 1;
    }
    assert!(checked >= 6, "the rule was barely exercised: {checked}");
}
