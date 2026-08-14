//! Byte-parity tests for [`crate::workspaces::plan::render_workspaces`], the block allocator
//! behind it, and [`crate::bar::workspaces::render_bar_workspaces`], against the real Python
//! backend.
//!
//! `testdata/workspace_fixtures.json` was captured during the Rust port by loading the former
//! backend with `SourceFileLoader`, planting a `displays.toml`, a `workspace-blocks.toml`, a
//! `preferences.toml` and sometimes an already-installed `workspaces.lua` into a scratch
//! `HOME`, monkeypatches `json_command()` so `hyprctl monitors -j` answers a scripted list,
//! then calls `render_workspaces()` and `render_bar_workspaces()` and reads back every byte
//! the two of them wrote.
//!
//! The matrix, 38 scenarios: both modes; `shared_count` at 1, 8 and 10; `default_count` at 1,
//! 4 and 10; counts strings that are clean, malformed, clamped and naming an absent output;
//! layouts that are absent, unreadable, wrongly shaped, disabled, mirrored, impossibly
//! mirrored, predating a live monitor, and naming one output twice; the primary coming from
//! the layout, from `HYPR_PRIMARY_MONITOR`, from neither, and naming a display that is not
//! there; a connector needing both Lua and JSON quoting; allocator files that are absent,
//! remembered, remembered out of order, holed, junk-valued, `[block]`-less and
//! `block`-not-a-table; and the empty plan with and without a fragment to take away.
//!
//! Three files are compared per scenario: `workspaces.lua` (or its absence),
//! `waybar-workspaces.jsonc`, and `workspace-blocks.toml` after the render -- the last being
//! the only layer-2 write a renderer makes, so its byte parity is the allocator's parity.

use std::collections::HashMap;
use std::path::Path;

use garage_core::paths::Paths;
use garage_core::schema::defaults::Defaults;
use garage_core::schema::notes::Notes;
use garage_core::schema::Preferences;
use garage_core::traits::{LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource};

use crate::bar::workspaces::render_bar_workspaces;
use crate::cx::RenderCx;
use crate::workspaces::plan::render_workspaces_for;

const FIXTURES: &str = include_str!("../testdata/workspace_fixtures.json");

/// The scripted answer to the one question a render may ask the compositor.
struct Scripted(Vec<Monitor>);

impl MonitorSource for Scripted {
    fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
        Ok(self.0.clone())
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
        "garage-workspace-parity-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let env: HashMap<String, String> = [("HOME".to_owned(), home.to_string_lossy().into_owned())]
        .into_iter()
        .collect();
    Paths::from_env_map(&env)
}

fn preferences_from_toml(departures: &str) -> Preferences {
    let table: toml::Table = departures.parse().expect("fixture toml parses");
    let defaults = Defaults::compiled().expect("shipped defaults parse");
    let mut notes = Notes::new();
    Preferences::coerce_from(&table, &defaults, &mut notes)
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    match value.get(key) {
        None | Some(serde_json::Value::Null) => None,
        Some(found) => Some(
            found
                .as_str()
                .unwrap_or_else(|| panic!("fixture field {key:?} is a string or null"))
                .to_owned(),
        ),
    }
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Rebuild one scenario's scratch `HOME`: the three files the Python planted, under the
/// paths this crate would read them from.
fn plant(paths: &Paths, expected: &serde_json::Value) {
    drop(std::fs::remove_dir_all(&paths.home));
    std::fs::create_dir_all(&paths.generated).expect("scratch state root");
    std::fs::create_dir_all(
        paths
            .host
            .displays
            .parent()
            .unwrap_or_else(|| Path::new("/")),
    )
    .expect("scratch config root");
    if let Some(layout) = text(expected, "displays_toml") {
        std::fs::write(&paths.host.displays, layout).expect("plant displays.toml");
    }
    if let Some(blocks) = text(expected, "blocks_toml_before") {
        std::fs::write(&paths.host.workspace_blocks, blocks).expect("plant workspace-blocks.toml");
    }
    if let Some(fragment) = text(expected, "pre_fragment") {
        std::fs::write(&paths.fragments.workspaces, fragment).expect("plant workspaces.lua");
    }
}

#[test]
fn every_scenario_matches_the_python_backend_byte_for_byte() {
    let all: serde_json::Value =
        serde_json::from_str(FIXTURES).expect("testdata/workspace_fixtures.json is valid JSON");
    let scenarios = all.as_object().expect("fixture root is an object");
    assert!(scenarios.len() >= 35, "the matrix should not have shrunk");

    for (name, expected) in scenarios {
        let prefs = preferences_from_toml(&text(expected, "prefs_toml").unwrap_or_default());
        let paths = scratch_paths(name);
        plant(&paths, expected);

        let monitors = Scripted(
            expected
                .get("monitors")
                .and_then(serde_json::Value::as_array)
                .expect("fixture names its monitors")
                .iter()
                .map(|value| Monitor {
                    name: value.as_str().unwrap_or_default().to_owned(),
                    width: 1920,
                    height: 1080,
                })
                .collect(),
        );
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);
        let primary = text(expected, "primary_env").unwrap_or_default();

        render_workspaces_for(&cx, &primary)
            .unwrap_or_else(|error| panic!("{name}: render_workspaces failed: {error}"));
        render_bar_workspaces(&cx)
            .unwrap_or_else(|error| panic!("{name}: render_bar_workspaces failed: {error}"));

        assert_eq!(
            read(&paths.fragments.workspaces),
            text(expected, "workspaces_lua"),
            "{name}: workspaces.lua"
        );
        assert_eq!(
            read(&paths.fragments.waybar_workspaces),
            text(expected, "waybar_workspaces_jsonc"),
            "{name}: waybar-workspaces.jsonc"
        );
        assert_eq!(
            read(&paths.host.workspace_blocks),
            text(expected, "blocks_toml_after"),
            "{name}: workspace-blocks.toml"
        );

        drop(std::fs::remove_dir_all(&paths.home));
    }
}

/// The invariant stated as a property rather than as bytes: an allocator file that already
/// names a display keeps that display's block whatever else happens to the ordering.
///
/// The fixtures cover the specific cases; this covers the rule they are cases of, over every
/// scenario that arrived with a remembered file.
#[test]
fn a_remembered_block_is_never_taken_away_from_its_display() {
    let all: serde_json::Value =
        serde_json::from_str(FIXTURES).expect("testdata/workspace_fixtures.json is valid JSON");
    let mut checked = 0;
    for (name, scenario) in all.as_object().expect("fixture root is an object") {
        let (Some(before), Some(after)) = (
            text(scenario, "blocks_toml_before"),
            text(scenario, "blocks_toml_after"),
        ) else {
            continue;
        };
        let before: toml::Table = before.parse().expect("the planted file parses");
        let after: toml::Table = after.parse().expect("the written file parses");
        let (Some(before), Some(after)) = (
            before.get("block").and_then(toml::Value::as_table),
            after.get("block").and_then(toml::Value::as_table),
        ) else {
            continue;
        };
        for (output, index) in before {
            // Only entries the loader would have kept: a junk value was dropped on the way
            // in, and the display it named is legitimately given a block again.
            if matches!(index, toml::Value::Integer(number) if *number >= 0) {
                assert_eq!(
                    after.get(output),
                    Some(index),
                    "{name}: {output} lost or moved the block it was remembered at"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 8,
        "the invariant was barely exercised: {checked}"
    );
}
