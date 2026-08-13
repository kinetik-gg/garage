//! The generated-comparison test: every table [`crate::palette::table`] and
//! [`crate::palette::accents`] carry, walked against a fixture dumped straight from the real
//! Python backend (see `testdata/palette_parity.json` and the throwaway dump script whose
//! output it is). Both directions are checked -- nothing missing from the Rust side, nothing
//! extra -- so a role added, renamed or dropped in either language fails here rather than at
//! the next login.
//!
//! A separate file from `table.rs` on purpose: this crate's line budget is per file, and the
//! comparison harness below is nearly as long as the tables it checks.

#![cfg(test)]

use crate::palette::accents::{border_colors, ACCENTS};
use crate::palette::table::{role, GTK3_TOKENS, GTK4_TOKENS, PALETTE, QT_ROLES};
use garage_core::schema::enums::Scheme;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const FIXTURE: &str = include_str!("../../testdata/palette_parity.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("testdata/palette_parity.json is valid JSON")
}

/// `value[key]`, without the indexing operator `indexing_slicing` denies crate-wide.
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("fixture is missing {key:?}"))
}

#[test]
fn palette_matches_the_python_dump() {
    let expected = fixture();
    let palette = field(&expected, "PALETTE");
    for (scheme, key) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
        let want: BTreeMap<String, String> = field(palette, key)
            .as_object()
            .expect("PALETTE.<scheme> is an object")
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().expect("value is a string").to_owned()))
            .collect();
        let got: BTreeMap<String, String> = PALETTE
            .iter()
            .map(|&(name, _, _)| {
                (
                    name.to_owned(),
                    role(scheme, name).unwrap_or_default().to_owned(),
                )
            })
            .collect();
        assert_eq!(got, want, "PALETTE[{key:?}] disagrees with the Python dump");
    }
}

#[test]
fn accents_match_the_python_dump() {
    let expected = fixture();
    let want: BTreeMap<String, String> = field(&expected, "ACCENTS")
        .as_object()
        .expect("ACCENTS is an object")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().expect("value is a string").to_owned()))
        .collect();
    let got: BTreeMap<String, String> = ACCENTS
        .iter()
        .map(|(name, hex)| ((*name).to_owned(), (*hex).to_owned()))
        .collect();
    assert_eq!(got, want, "ACCENTS disagrees with the Python dump");
}

#[test]
fn border_colors_match_the_python_dump() {
    let expected = fixture();
    let border_colors_fixture = field(&expected, "BORDER_COLORS");
    for (scheme, key) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
        let want = field(border_colors_fixture, key)
            .as_array()
            .expect("BORDER_COLORS.<scheme> is an array");
        let want_active = want
            .first()
            .and_then(Value::as_str)
            .expect("active is a string");
        let want_inactive = want
            .get(1)
            .and_then(Value::as_str)
            .expect("inactive is a string");
        let (active, inactive) = border_colors(scheme);
        assert_eq!(active, want_active, "BORDER_COLORS[{key:?}].0 disagrees");
        assert_eq!(
            inactive, want_inactive,
            "BORDER_COLORS[{key:?}].1 disagrees"
        );
    }
}

#[test]
fn gtk3_tokens_match_the_python_dump() {
    let expected = fixture();
    let want: Vec<(String, String)> = field(&expected, "GTK3_TOKENS")
        .as_array()
        .expect("GTK3_TOKENS is an array")
        .iter()
        .map(pair_of_strings)
        .collect();
    let got: Vec<(String, String)> = GTK3_TOKENS
        .iter()
        .map(|(t, r)| ((*t).to_owned(), (*r).to_owned()))
        .collect();
    assert_eq!(
        got, want,
        "GTK3_TOKENS disagrees with the Python dump (order matters here)"
    );
}

#[test]
fn gtk4_tokens_match_the_python_dump() {
    let expected = fixture();
    let want: Vec<(String, String)> = field(&expected, "GTK4_TOKENS")
        .as_array()
        .expect("GTK4_TOKENS is an array")
        .iter()
        .map(pair_of_strings)
        .collect();
    let got: Vec<(String, String)> = GTK4_TOKENS
        .iter()
        .map(|(t, r)| ((*t).to_owned(), (*r).to_owned()))
        .collect();
    assert_eq!(
        got, want,
        "GTK4_TOKENS disagrees with the Python dump (order matters here)"
    );
}

/// A `["a", "b"]` JSON pair as an owned `(String, String)`, without indexing.
fn pair_of_strings(pair: &Value) -> (String, String) {
    let pair = pair.as_array().expect("entry is a pair");
    let mut strings = pair
        .iter()
        .map(|v| v.as_str().expect("entry is a string").to_owned());
    let first = strings.next().expect("pair has a first element");
    let second = strings.next().expect("pair has a second element");
    (first, second)
}

#[test]
fn qt_roles_match_the_python_dump() {
    let expected = fixture();
    let want: Vec<Vec<String>> = field(&expected, "QT_ROLES")
        .as_array()
        .expect("QT_ROLES is an array")
        .iter()
        .map(|row| {
            row.as_array()
                .expect("entry is a row")
                .iter()
                .map(|v| v.as_str().expect("entry is a string").to_owned())
                .collect()
        })
        .collect();
    let got: Vec<Vec<String>> = QT_ROLES
        .iter()
        .map(|(role, active, inactive, disabled)| {
            vec![
                (*role).to_owned(),
                (*active).to_owned(),
                (*inactive).to_owned(),
                (*disabled).to_owned(),
            ]
        })
        .collect();
    assert_eq!(
        got, want,
        "QT_ROLES disagrees with the Python dump (order matters here)"
    );
}

#[test]
fn every_role_is_defined_for_both_schemes() {
    // A role defined for one appearance only is a `KeyError` mid-render in the Python
    // original; here it is simply impossible by construction, since every PALETTE row
    // always carries both a light and a dark value -- this test instead pins that PALETTE
    // names exactly the roles the fixture does, so a role dropped from one side is still
    // caught.
    let expected = fixture();
    let palette = field(&expected, "PALETTE");
    let dark_object = field(palette, "dark").as_object().expect("object");
    let light_object = field(palette, "light").as_object().expect("object");
    let dark_keys: BTreeSet<&str> = dark_object.keys().map(String::as_str).collect();
    let light_keys: BTreeSet<&str> = light_object.keys().map(String::as_str).collect();
    assert_eq!(dark_keys, light_keys);
    let ours: BTreeSet<&str> = PALETTE.iter().map(|&(name, _, _)| name).collect();
    assert_eq!(ours, dark_keys);
}
