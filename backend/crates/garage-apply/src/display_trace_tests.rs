//! Trace-parity tests for the display transaction: `display_test()`, `display_finish()` and
//! `initialize_display_config()`, against the real Python backend.
//!
//! `testdata/display_traces.json` was captured during the Rust port by loading the former
//! backend, planting a `displays.toml` into a scratch `HOME`, and replacing its `run()` with a
//! recorder that answers `hyprctl monitors all -j` with scripted JSON and `hyprctl reload`
//! with a scripted status, replaces `subprocess.Popen` with one that records the watchdog's
//! argv instead of forking it, and then drives the whole choreography.
//!
//! `luac -p` is elided from both sides, as it is in the workspace trace corpus: the Rust side
//! reaches Lua through [`LuaSyntaxCheck`](garage_core::traits::LuaSyntaxCheck) rather than
//! through the runner, and the fragment's bytes are already pinned byte for byte by
//! `garage-render`'s own display corpus.
//!
//! What the scenarios pin down: `display-test` followed by confirm, by revert, and by the
//! watchdog (which *is* `display_finish(token, False)`, and is here to say so); a test against
//! a machine with no saved layout; a confirm carrying the wrong token; a confirm and a revert
//! with nothing pending at all; a mirrored layout and one at two different scales; every
//! refusal `apply_display_layout()` can produce -- overlap, gap, only-mirrors, an impossible
//! mirror, nothing enabled, an empty layout -- and a reload that fails both with and without
//! something to say. Then the seeding half: `initialize_display_config()` against a machine
//! with no file, with a file, with an *empty* file, with no compositor answering, with one
//! display, with nothing focused, with a mirror and with a disabled output.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use garage_core::paths::Paths;
use garage_core::schema::defaults::Defaults;
use garage_core::traits::{LuaCheckError, LuaSyntaxCheck, Output, RunError, Runner};
use garage_proc::Hyprctl;
use garage_render::cx::RenderCx;
use serde_json::Value;

use crate::cx::SessionCx;
use crate::displays::transaction::{display_finish, display_test, initialize_display_config};

const TRACES: &str = include_str!("../testdata/display_traces.json");

/// The Python's `run` shim: records the argv, answers the monitor read and the reload.
struct Recorder {
    monitors: String,
    reload_status: i32,
    reload_stderr: String,
    calls: RefCell<Vec<Vec<String>>>,
    spawned: RefCell<Vec<Vec<String>>>,
}

impl Runner for Recorder {
    fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
        self.calls
            .borrow_mut()
            .push(command.iter().map(|part| (*part).to_owned()).collect());
        match command {
            ["hyprctl", "monitors", "all", "-j", ..] => Ok(Output {
                status: 0,
                stdout: self.monitors.clone(),
                stderr: String::new(),
            }),
            ["hyprctl", "reload"] => Ok(Output {
                status: self.reload_status,
                stdout: String::new(),
                stderr: self.reload_stderr.clone(),
            }),
            _ => Ok(Output {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }),
        }
    }

    fn spawn_detached(&self, command: &[&str]) -> Result<(), RunError> {
        self.spawned
            .borrow_mut()
            .push(command.iter().map(|part| (*part).to_owned()).collect());
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
        "garage-display-trace-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let env: HashMap<String, String> = [("HOME".to_owned(), home.to_string_lossy().into_owned())]
        .into_iter()
        .collect();
    Paths::from_env_map(&env)
}

fn text(value: &Value, key: &str) -> Option<String> {
    match value.get(key) {
        None | Some(Value::Null) => None,
        Some(found) => found.as_str().map(str::to_owned),
    }
}

fn argv_list(scenario: &Value, key: &str) -> Vec<Vec<String>> {
    scenario
        .get(key)
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .map(|call| {
                    call.as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .map(|part| part.as_str().unwrap_or_default().to_owned())
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The Python records `f"{type(error).__name__}: {error}"`; the port has one error type and
/// only the message is contract.
fn without_exception_name(recorded: &str) -> &str {
    recorded.split_once(": ").map_or(recorded, |(_, rest)| rest)
}

/// Both sides refused, or neither did, and the message is the Python's.
fn assert_refusal(outcome: Result<(), String>, expected: &str, name: &str) {
    match (outcome, expected.is_empty()) {
        (Ok(()), true) => {}
        (Err(error), false) => assert_eq!(error, without_exception_name(expected), "{name}"),
        (Ok(()), false) => panic!("{name}: the Python refused this and the port did not"),
        (Err(error), true) => panic!("{name}: the port refused and the Python did not: {error}"),
    }
}

fn recorder_for(scenario: &Value) -> Recorder {
    Recorder {
        monitors: scenario
            .get("monitors")
            .map_or_else(|| "[]".to_owned(), ToString::to_string),
        reload_status: i32::try_from(
            scenario
                .get("reload_status")
                .and_then(Value::as_i64)
                .unwrap_or(0),
        )
        .unwrap_or(0),
        reload_stderr: text(scenario, "reload_stderr").unwrap_or_default(),
        calls: RefCell::new(Vec::new()),
        spawned: RefCell::new(Vec::new()),
    }
}

fn plant(paths: &Paths, scenario: &Value) {
    drop(std::fs::remove_dir_all(&paths.home));
    std::fs::create_dir_all(&paths.generated).expect("scratch state root");
    std::fs::create_dir_all(&paths.root).expect("scratch config root");
    if let Some(saved) = text(scenario, "saved_before") {
        std::fs::write(&paths.host.displays, saved).expect("plant displays.toml");
    }
}

#[test]
fn every_flow_issues_the_same_commands_and_leaves_the_same_files() {
    let all: Value =
        serde_json::from_str(TRACES).expect("testdata/display_traces.json is valid JSON");
    let scenarios = all.as_object().expect("trace fixture root is an object");
    let defaults = Defaults::compiled().expect("shipped defaults parse");
    let mut flows = 0;

    for (name, scenario) in scenarios {
        if name.starts_with("seed_") {
            continue;
        }
        flows += 1;
        let paths = scratch_paths(name);
        plant(&paths, scenario);
        let recorder = recorder_for(scenario);
        let hyprctl = Hyprctl::new(&recorder);
        let lua = LuaAccepts;
        let cx = SessionCx::new(
            RenderCx::new(defaults.values(), &paths, &hyprctl, &lua),
            &recorder as &dyn Runner,
        );

        let (errors, token) = drive(&cx, scenario);

        let expected: Vec<String> = scenario
            .get("errors")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        without_exception_name(item.as_str().unwrap_or_default()).to_owned()
                    })
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(errors, expected, "{name}: refusals");
        assert_eq!(
            recorder.calls.borrow().clone(),
            argv_list(scenario, "trace"),
            "{name}: trace"
        );
        assert_watchdog(&recorder, scenario, &token, name);
        assert_left_behind(&paths, scenario, name);
        drop(std::fs::remove_dir_all(&paths.home));
    }
    assert!(flows >= 15, "the flow matrix should not have shrunk");
}

/// How many watchdogs were spawned, and with what argv.
///
/// The argv is compared without its program name and without the token: the program name is
/// `sys.executable`'s script path there and `current_exe()` here, and the token is fresh on
/// every run. See this task's `deviations.toml` entry.
fn assert_watchdog(recorder: &Recorder, scenario: &Value, token: &str, name: &str) {
    assert_eq!(
        recorder.spawned.borrow().len(),
        usize::try_from(scenario.get("spawned").and_then(Value::as_i64).unwrap_or(0)).unwrap_or(0),
        "{name}: watchdogs spawned"
    );
    let tail: Vec<String> = recorder
        .spawned
        .borrow()
        .first()
        .map(|argv| {
            argv.iter()
                .skip(1)
                .filter(|part| *part != token)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let expected: Vec<String> = scenario
        .get("spawn_tail")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .map(|part| part.as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(tail, expected, "{name}: the watchdog's argv");
}

/// One scenario's `display-test`, and the confirm, revert or watchdog that follows it.
/// Hands back every refusal in the order it happened, and the token the test produced.
fn drive(cx: &SessionCx<'_>, scenario: &Value) -> (Vec<String>, String) {
    let mut errors: Vec<String> = Vec::new();
    let mut token = String::new();
    if let Some(payload) = scenario.get("payload").filter(|held| !held.is_null()) {
        match display_test(cx, payload, "") {
            Ok(fresh) => token = fresh,
            Err(error) => errors.push(error.to_string()),
        }
    }
    if let Some(then) = text(scenario, "then") {
        let handed = if then == "wrong-token" {
            "not-the-token"
        } else {
            &token
        };
        if let Err(error) = display_finish(cx, handed, then == "confirm") {
            errors.push(error.to_string());
        }
    }
    (errors, token)
}

/// The three files a flow can leave behind: the saved layout, the fragment, and whether a
/// transaction is still open.
fn assert_left_behind(paths: &Paths, scenario: &Value, name: &str) {
    assert_eq!(
        std::fs::read_to_string(&paths.host.displays).ok(),
        text(scenario, "displays_toml_after"),
        "{name}: displays.toml afterwards"
    );
    assert_eq!(
        std::fs::read_to_string(&paths.fragments.displays).ok(),
        text(scenario, "displays_lua_after"),
        "{name}: displays.lua afterwards"
    );
    assert_eq!(
        paths.pending_display.exists(),
        scenario
            .get("pending_left_behind")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "{name}: the pending transaction"
    );
}

/// `initialize_display_config()` on its own: the seed-once semantics, including the claim the
/// whole function turns on -- a file that already exists is left byte for byte alone.
#[test]
fn the_seed_writes_once_and_never_overwrites() {
    let all: Value =
        serde_json::from_str(TRACES).expect("testdata/display_traces.json is valid JSON");
    let defaults = Defaults::compiled().expect("shipped defaults parse");
    let mut seeds = 0;

    for (name, scenario) in all.as_object().expect("trace fixture root is an object") {
        if !name.starts_with("seed_") {
            continue;
        }
        seeds += 1;
        let paths = scratch_paths(name);
        plant(&paths, scenario);
        let recorder = recorder_for(scenario);
        let hyprctl = Hyprctl::new(&recorder);
        let lua = LuaAccepts;
        let cx = SessionCx::new(
            RenderCx::new(defaults.values(), &paths, &hyprctl, &lua),
            &recorder as &dyn Runner,
        );

        let outcome = initialize_display_config(&cx, "");
        assert_refusal(
            outcome.map_err(|error| error.to_string()),
            &text(scenario, "error").unwrap_or_default(),
            name,
        );
        assert_eq!(
            recorder.calls.borrow().clone(),
            argv_list(scenario, "trace"),
            "{name}: trace"
        );
        assert_eq!(
            std::fs::read_to_string(&paths.host.displays).ok(),
            text(scenario, "displays_toml_after"),
            "{name}: displays.toml afterwards"
        );
        // Seeding is a layer-2 write and nothing else: no fragment, no reload, no pending
        // transaction.
        assert!(
            !paths.fragments.displays.exists(),
            "{name}: wrote a fragment"
        );
        assert!(
            !paths.pending_display.exists(),
            "{name}: left a transaction"
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }
    assert!(seeds >= 8, "the seeding matrix should not have shrunk");
}

/// The seed-once claim as a property rather than as bytes: whatever a scenario planted, a
/// file that was already there comes back unchanged, and the read of the compositor that
/// would have produced a replacement never happens at all.
#[test]
fn an_existing_layout_is_not_even_looked_past() {
    let defaults = Defaults::compiled().expect("shipped defaults parse");
    let paths = scratch_paths("seed-once-property");
    drop(std::fs::remove_dir_all(&paths.home));
    std::fs::create_dir_all(&paths.root).expect("scratch config root");
    let planted = "# hand written, keep me\nprimary = \"DP-9\"\n";
    std::fs::write(&paths.host.displays, planted).expect("plant displays.toml");

    let recorder = Recorder {
        monitors: "[]".to_owned(),
        reload_status: 0,
        reload_stderr: String::new(),
        calls: RefCell::new(Vec::new()),
        spawned: RefCell::new(Vec::new()),
    };
    let hyprctl = Hyprctl::new(&recorder);
    let lua = LuaAccepts;
    let cx = SessionCx::new(
        RenderCx::new(defaults.values(), &paths, &hyprctl, &lua),
        &recorder as &dyn Runner,
    );

    initialize_display_config(&cx, "").expect("the seed is a no-op over an existing file");
    assert_eq!(
        std::fs::read_to_string(&paths.host.displays).ok(),
        Some(planted.to_owned()),
        "the user's own file was rewritten"
    );
    assert!(
        recorder.calls.borrow().is_empty(),
        "the compositor was asked despite the file already existing"
    );
    assert!(
        !paths.locks.display.exists(),
        "the lock was taken despite the file already existing"
    );
    drop(std::fs::remove_dir_all(&paths.home));
}
