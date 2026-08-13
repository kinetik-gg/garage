//! Trace-parity tests for [`crate::workspaces::apply_workspace_plan`]: the exact commands it
//! issues, in the exact order, against the Python's own.
//!
//! `testdata/workspace_traces.json` is the output of a throwaway generator (not committed --
//! see this task's report) that loads `desktop/.local/bin/garage` through `tests/harness.py`,
//! plants a `displays.toml`, a `workspace-blocks.toml`, a `preferences.toml` and an already
//! installed `workspaces.lua` into a scratch `HOME`, replaces the backend's own `run()` with
//! a shim that records every argv and answers `hyprctl monitors -j` / `hyprctl clients -j`
//! with scripted JSON, and then calls `apply_workspace_plan()`.
//!
//! The Rust side is given the same planted files, the same scripted answers, and one
//! recorder that backs *both* the [`Runner`] and the [`MonitorSource`] -- through
//! [`garage_proc::Hyprctl`], which is what production uses -- so the `hyprctl monitors -j`
//! calls the render half makes land in the same trace the Python's did. Only `luac -p` is
//! elided from both sides: the Rust side reaches Lua through
//! [`LuaSyntaxCheck`](garage_core::traits::LuaSyntaxCheck) rather than the runner, and the
//! candidate path it is handed is a fresh temporary name on every run.
//!
//! What each scenario pins down: shared mode signalling nothing but the reload; an empty plan
//! taking the fragment away; a steady state moving nothing; a lowered count reaping the
//! surplus to the last surviving slot; a moved block remapping window by window *before* the
//! reap reads the clients a second time; the scratchpad's negative ids surviving untouched;
//! an id in no display's block being left where it is; a display showing a dropped slot being
//! sent to its own last slot and the focus being handed back; a display that arrived
//! mid-flight keeping what it is on; and the two fragments that are no source to map from --
//! an absent one and a shared one.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use garage_core::paths::Paths;
use garage_core::schema::defaults::Defaults;
use garage_core::schema::notes::Notes;
use garage_core::schema::Preferences;
use garage_core::traits::{LuaCheckError, LuaSyntaxCheck, Output, RunError, Runner};
use garage_proc::Hyprctl;
use garage_render::cx::RenderCx;

use crate::cx::SessionCx;
use crate::workspaces::apply_workspace_plan;

const TRACES: &str = include_str!("../testdata/workspace_traces.json");

/// The Python's `run` shim: records the argv, answers the two `-j` reads and nothing else.
struct Recorder {
    monitors: String,
    clients: String,
    calls: RefCell<Vec<Vec<String>>>,
}

impl Runner for Recorder {
    fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
        self.calls
            .borrow_mut()
            .push(command.iter().map(|part| (*part).to_owned()).collect());
        let stdout = match command {
            ["hyprctl", "monitors", "-j", ..] => self.monitors.clone(),
            ["hyprctl", "clients", "-j", ..] => self.clients.clone(),
            _ => String::new(),
        };
        Ok(Output {
            status: 0,
            stdout,
            stderr: String::new(),
        })
    }

    fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
        Ok(())
    }

    fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
        Ok(0)
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
        "garage-workspace-trace-{label}-{}-{:?}",
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

fn preferences_from_toml(departures: &str) -> Preferences {
    let table: toml::Table = departures.parse().expect("fixture toml parses");
    let defaults = Defaults::compiled().expect("shipped defaults parse");
    let mut notes = Notes::new();
    Preferences::coerce_from(&table, &defaults, &mut notes)
}

/// The argv list the Python's shim recorded, as owned strings.
fn expected_trace(scenario: &serde_json::Value) -> Vec<Vec<String>> {
    scenario
        .get("trace")
        .and_then(serde_json::Value::as_array)
        .expect("fixture carries a trace")
        .iter()
        .map(|call| {
            call.as_array()
                .expect("each trace entry is an argv")
                .iter()
                .map(|part| part.as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .collect()
}

/// Rebuild one scenario's scratch `HOME`: the layout, the allocator file and the fragment
/// the Python had on disk when it was recorded.
fn plant(paths: &Paths, scenario: &serde_json::Value) {
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
    if let Some(layout) = text(scenario, "displays_toml") {
        std::fs::write(&paths.host.displays, layout).expect("plant displays.toml");
    }
    if let Some(blocks) = text(scenario, "blocks_toml_before") {
        std::fs::write(&paths.host.workspace_blocks, blocks).expect("plant the blocks file");
    }
    if let Some(fragment) = text(scenario, "fragment_before") {
        std::fs::write(&paths.fragments.workspaces, fragment).expect("plant workspaces.lua");
    }
}

#[test]
fn every_scenario_issues_the_same_commands_in_the_same_order() {
    let all: serde_json::Value =
        serde_json::from_str(TRACES).expect("testdata/workspace_traces.json is valid JSON");
    let scenarios = all.as_object().expect("trace fixture root is an object");
    assert!(scenarios.len() >= 10, "the matrix should not have shrunk");

    for (name, scenario) in scenarios {
        let prefs = preferences_from_toml(&text(scenario, "prefs_toml").unwrap_or_default());
        let paths = scratch_paths(name);
        plant(&paths, scenario);

        let recorder = Recorder {
            monitors: scenario
                .get("monitors")
                .expect("fixture names its monitors")
                .to_string(),
            clients: scenario
                .get("clients")
                .expect("fixture names its clients")
                .to_string(),
            calls: RefCell::new(Vec::new()),
        };
        let hyprctl = Hyprctl::new(&recorder);
        let lua = LuaAccepts;
        let mut cx = SessionCx::new(
            RenderCx::new(&prefs, &paths, &hyprctl, &lua),
            &recorder as &dyn Runner,
        );

        apply_workspace_plan(&mut cx)
            .unwrap_or_else(|error| panic!("{name}: apply_workspace_plan failed: {error}"));

        assert_eq!(
            recorder.calls.borrow().clone(),
            expected_trace(scenario),
            "{name}: trace"
        );

        assert_eq!(
            std::fs::read_to_string(&paths.fragments.workspaces).ok(),
            text(scenario, "workspaces_lua_after"),
            "{name}: workspaces.lua after the apply"
        );

        drop(std::fs::remove_dir_all(&paths.home));
    }
}

/// The pattern scraper, against the shapes `installed_workspace_groups()` has to survive:
/// the fragment as generated, the same fragment reformatted onto one line, one with padding
/// the generator would never emit, and text that names no group at all.
#[test]
fn the_installed_group_pattern_reads_a_reformatted_fragment() {
    use crate::workspaces::installed::scan_installed_groups;

    let generated = "-- Generated by garage. Edit preferences.toml instead.\n\
                     WORKSPACE_PLAN = {\n    mode = \"per-display\",\n    groups = {\n\
                     \x20       { monitor = \"DP-1\", first = 1, count = 8 },\n\
                     \x20       { monitor = \"HDMI-A-1\", first = 21, count = 4 },\n\
                     \x20   },\n}\n";
    assert_eq!(
        scan_installed_groups(generated),
        vec![("DP-1".to_owned(), 1, 8), ("HDMI-A-1".to_owned(), 21, 4)]
    );

    assert_eq!(
        scan_installed_groups("{monitor=\"eDP-1\",first=11,count=3}"),
        vec![("eDP-1".to_owned(), 11, 3)]
    );
    assert_eq!(
        scan_installed_groups("monitor  =  \"\"  ,  first  =  1  ,  count  =  10"),
        vec![(String::new(), 1, 10)]
    );
    assert!(scan_installed_groups("WORKSPACE_PLAN = {}").is_empty());
    // A partial match must not swallow the real one that follows it.
    assert_eq!(
        scan_installed_groups(
            "monitor = \"X\", first = , count = 1 monitor = \"Y\", first = 2, count = 3"
        ),
        vec![("Y".to_owned(), 2, 3)]
    );
}

/// `surviving_slots()` and `block_of()` on their own, since both are read twice by the
/// choreography above and a wrong answer from either is invisible in a trace that then moves
/// nothing.
#[test]
fn the_surviving_slots_are_the_ids_the_plan_keeps_and_the_last_of_each_block() {
    use garage_core::schema::enums::WorkspaceMode;
    use garage_core::schema::newtypes::BlockId;
    use garage_render::workspaces::{WorkspaceGroup, WorkspacePlan};

    use crate::workspaces::salvage::{block_of, surviving_slots};

    assert_eq!(block_of(1), 0);
    assert_eq!(block_of(10), 0);
    assert_eq!(block_of(11), 1);
    assert_eq!(block_of(21), 2);
    assert_eq!(block_of(97), 9);

    let plan = WorkspacePlan {
        mode: WorkspaceMode::PerDisplay,
        groups: vec![
            WorkspaceGroup {
                monitor: "DP-1".into(),
                first: BlockId::new(0).first(),
                count: 3,
            },
            WorkspaceGroup {
                monitor: "DP-2".into(),
                first: BlockId::new(2).first(),
                count: 2,
            },
        ],
    };
    let (keep, last) = surviving_slots(&plan);
    assert_eq!(keep, vec![1, 2, 3, 21, 22]);
    assert_eq!(last, vec![(0, 3), (2, 22)]);
}
