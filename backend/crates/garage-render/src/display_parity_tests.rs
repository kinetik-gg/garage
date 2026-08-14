//! Byte-parity tests for [`crate::displays`]: the saved-layout reader, the mirror rules and
//! the generated `displays.lua`, against the real Python backend.
//!
//! `testdata/display_fixtures.json` was captured during the Rust port by loading the former
//! backend with `SourceFileLoader`, planting a `displays.toml` (and sometimes an installed
//! `displays.lua`) into a scratch `HOME`, runs the exact four lines `render_all()` ends with
//! -- `layout = load_display_config()`, then `render_displays(layout)` or
//! `DISPLAY_FRAGMENT.unlink(missing_ok=True)` -- and reads back the fragment or the refusal.
//!
//! The matrix, 50 scenarios: the three empty shapes that take the fragment away, each also
//! run over a stale fragment; a record with nothing but an output and one with every field
//! set; the primary present, absent, empty, non-string, and naming a display that is not
//! there; disabled displays, including `enabled` spelled as a false-y `0` and `""` and a
//! truthy `"no"`; scales whole, fractional, integer, over-long and given as a string, plus
//! one that is not a number at all; VRR below, above and inside Hyprland's `-1..3`, and one
//! given as a float; coordinates as floats, as strings and as nonsense; every branch of
//! `mirror_targets()` -- an honoured mirror, one written ahead of its source, a display
//! mirroring itself, a mirror of a disabled output, of an absent output and of another
//! mirror, an empty mirror string, and two displays mirroring one; records with no `output`
//! key at all; an output needing Lua quoting and one carrying a newline; the same output
//! twice; and the malformed files -- `display` not an array, `display` a table, and a file
//! that is not TOML.

use std::collections::HashMap;
use std::path::Path;

use garage_core::paths::Paths;
use garage_core::schema::defaults::Defaults;
use garage_core::traits::{LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource};

use crate::cx::RenderCx;
use crate::displays::{load_display_config, mirror_targets, render_saved_displays, DisplayEntry};

const FIXTURES: &str = include_str!("../testdata/display_fixtures.json");

struct NoMonitors;
impl MonitorSource for NoMonitors {
    fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
        Ok(vec![])
    }
}

struct LuaAccepts;
impl LuaSyntaxCheck for LuaAccepts {
    fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
        Ok(())
    }
}

fn scratch_paths(label: &str) -> Paths {
    let home = std::env::temp_dir().join(format!(
        "garage-display-parity-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let env: HashMap<String, String> = [("HOME".to_owned(), home.to_string_lossy().into_owned())]
        .into_iter()
        .collect();
    Paths::from_env_map(&env)
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    match value.get(key) {
        None | Some(serde_json::Value::Null) => None,
        Some(found) => found.as_str().map(str::to_owned),
    }
}

/// The Python records `f"{type(error).__name__}: {error}"`; the Rust side has one error type
/// and only the message is contract. `tomllib` and the `toml` crate word a *syntax* error
/// differently and neither wording is depended on -- see [`crate::error::RenderError::Toml`]
/// -- so those are compared as "both refused" rather than word for word.
fn is_parser_wording(expected: &str) -> bool {
    expected.starts_with("SettingsError: displays.toml: ")
        && expected != "SettingsError: displays.toml: display must be an array of tables"
}

#[test]
fn every_scenario_matches_the_python_backend_byte_for_byte() {
    let all: serde_json::Value =
        serde_json::from_str(FIXTURES).expect("testdata/display_fixtures.json is valid JSON");
    let scenarios = all.as_object().expect("fixture root is an object");
    assert!(scenarios.len() >= 45, "the matrix should not have shrunk");

    let defaults = Defaults::compiled().expect("shipped defaults parse");
    for (name, expected) in scenarios {
        let paths = scratch_paths(name);
        drop(std::fs::remove_dir_all(&paths.home));
        std::fs::create_dir_all(&paths.root).expect("scratch config root");
        std::fs::create_dir_all(&paths.generated).expect("scratch state root");
        if let Some(layout) = text(expected, "displays_toml") {
            std::fs::write(&paths.host.displays, layout).expect("plant displays.toml");
        }
        if let Some(stale) = text(expected, "pre_fragment") {
            std::fs::write(&paths.fragments.displays, stale).expect("plant a stale fragment");
        }

        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);
        let outcome = render_saved_displays(&cx);

        let wanted = text(expected, "error").unwrap_or_default();
        match (&outcome, wanted.is_empty()) {
            (Ok(()), true) => {}
            (Err(error), false) => {
                if !is_parser_wording(&wanted) {
                    let (_, message) = wanted
                        .split_once(": ")
                        .expect("the Python's record carries an exception name");
                    assert_eq!(error.to_string(), message, "{name}: refusal");
                }
            }
            (Ok(()), false) => panic!("{name}: the Python refused this and the port did not"),
            (Err(error), true) => {
                panic!("{name}: the port refused this and the Python did not: {error}")
            }
        }

        assert_eq!(
            std::fs::read_to_string(&paths.fragments.displays).ok(),
            text(expected, "displays_lua"),
            "{name}: displays.lua"
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }
}

/// The rule the mirror scenarios above are cases of, stated once: a display only ever mirrors
/// an enabled display that is not itself a mirror, and never itself.
#[test]
fn an_honoured_mirror_always_names_an_enabled_non_mirror_that_is_not_itself() {
    let all: serde_json::Value =
        serde_json::from_str(FIXTURES).expect("testdata/display_fixtures.json is valid JSON");
    let mut checked = 0;
    for (name, scenario) in all.as_object().expect("fixture root is an object") {
        let Some(layout) = text(scenario, "displays_toml") else {
            continue;
        };
        let paths = scratch_paths(&format!("rule-{name}"));
        drop(std::fs::remove_dir_all(&paths.home));
        std::fs::create_dir_all(&paths.root).expect("scratch config root");
        std::fs::write(&paths.host.displays, layout).expect("plant displays.toml");
        let Ok(saved) = load_display_config(&paths.host.displays) else {
            drop(std::fs::remove_dir_all(&paths.home));
            continue;
        };
        for (output, source) in mirror_targets(&saved.displays) {
            assert_ne!(output, source, "{name}: a display mirrors itself");
            let their_mirror = saved
                .displays
                .iter()
                .find(|entry| entry.output() == source && entry.enabled())
                .map(DisplayEntry::mirror);
            assert_eq!(
                their_mirror,
                Some(String::new()),
                "{name}: {output} mirrors {source}, which is off or a mirror itself"
            );
            checked += 1;
        }
        drop(std::fs::remove_dir_all(&paths.home));
    }
    assert!(checked >= 4, "the rule was barely exercised: {checked}");
}
