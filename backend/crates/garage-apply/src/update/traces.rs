//! `update`'s seven steps, as traces: what it ran, in what order, and what it printed.
//!
//! `testdata/update_traces.json` comes from a throwaway generator, as do the other trace
//! fixtures. Git and bootstrap are faked: update streams bootstrap even in a dry run, so a
//! trace test must never reach either real boundary.
//!
//! What is compared: the transcript, the exit status or the refusal message, every
//! captured command in order, and the streamed call -- its argv, its working directory,
//! and the two environment variables `update` sets around it, which is the half of the
//! delegation `bootstrap.sh` actually reads.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use garage_core::paths::Paths;
use garage_core::traits::{Output, RunError, Runner};
use serde_json::Value;

use crate::doctor::DoctorCx;

use super::lock::UpdateLock;
use super::space::FixedSpace;
use super::transcript::{binary_build, Report};
use super::{run_steps, update_at, UpdateRun};

const TRACES: &str = include_str!("../../testdata/update_traces.json");
static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// One streamed invocation, recorded rather than run.
#[derive(Debug, PartialEq, Eq)]
struct Streamed {
    command: Vec<String>,
    cwd: String,
    skip_plugin_deploy: String,
    force: String,
}

struct FakeRunner {
    git: HashMap<String, (i32, String)>,
    heads: RefCell<VecDeque<(i32, String)>>,
    hyprland: String,
    compositor: i32,
    bootstrap_status: i32,
    trace: RefCell<Vec<String>>,
    streamed: RefCell<Vec<Streamed>>,
}

impl Runner for FakeRunner {
    fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
        self.trace.borrow_mut().push(command.join(" "));
        let (status, stdout) = match command {
            ["git", "-C", _, "rev-parse", "HEAD"] => self
                .heads
                .borrow_mut()
                .pop_front()
                .or_else(|| self.git.get("rev-parse HEAD").cloned())
                .unwrap_or((0, String::new())),
            ["git", "-C", _, rest @ ..] => self
                .git
                .get(&rest.join(" "))
                .cloned()
                .unwrap_or((0, String::new())),
            ["Hyprland", "--version"] => {
                (i32::from(self.hyprland.is_empty()), self.hyprland.clone())
            }
            ["hyprctl", "version"] => (self.compositor, String::new()),
            _ => (0, String::new()),
        };
        Ok(Output {
            status,
            stdout,
            stderr: String::new(),
        })
    }

    fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
        unreachable!("update never detaches a child")
    }

    /// The environment is read *here*, inside the call, because that is the only moment it
    /// is set: `update` puts the two variables in place around the streamed call and takes
    /// them back off afterwards, so that the plugin rebuild -- which runs later in the same
    /// process -- sees exactly what the Python's own child sees.
    fn run_streamed(&self, command: &[&str], cwd: Option<&Path>) -> Result<i32, RunError> {
        self.streamed.borrow_mut().push(Streamed {
            command: command.iter().map(|part| (*part).to_owned()).collect(),
            cwd: cwd
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            skip_plugin_deploy: std::env::var("GARAGE_SKIP_PLUGIN_DEPLOY").unwrap_or_default(),
            force: std::env::var("GARAGE_FORCE").unwrap_or_default(),
        });
        Ok(self.bootstrap_status)
    }
}

struct World {
    root: PathBuf,
    checkout: PathBuf,
    home: PathBuf,
    plugins: PathBuf,
}

impl World {
    fn expand(&self, text: &str) -> String {
        text.replace("$CHECKOUT", &self.checkout.to_string_lossy())
            .replace(
                "$OTHER",
                &self.root.join("other-checkout").to_string_lossy(),
            )
            .replace("$HOME", &self.home.to_string_lossy())
            .replace("$PLUGINS", &self.plugins.to_string_lossy())
    }

    fn normalize(&self, text: &str) -> String {
        text.replace(&self.checkout.to_string_lossy().into_owned(), "$CHECKOUT")
            .replace(
                &self
                    .root
                    .join("other-checkout")
                    .to_string_lossy()
                    .into_owned(),
                "$OTHER",
            )
            .replace(&self.home.to_string_lossy().into_owned(), "$HOME")
            .replace(&self.plugins.to_string_lossy().into_owned(), "$PLUGINS")
            .replace(&binary_build(), "$GARAGE_BINARY")
    }

    fn plant(&self, base: &Path, spec: &Value) {
        let Some(table) = spec.as_object() else {
            return;
        };
        for (relative, content) in table {
            self.plant_one(&base.join(relative), content);
        }
    }

    /// One entry: `["symlink", target]` becomes a link, anything else becomes a file.
    fn plant_one(&self, target: &Path, content: &Value) {
        Self::parents(target);
        let link = content
            .as_array()
            .filter(|pair| pair.first() == Some(&Value::String("symlink".to_owned())))
            .and_then(|pair| pair.get(1))
            .and_then(Value::as_str);
        match link {
            Some(where_) => Self::link(&self.expand(where_), target),
            None => {
                std::fs::write(target, content.as_str().unwrap_or_default()).expect("write");
            }
        }
    }

    fn parents(target: &Path) {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
    }

    fn link(target: &str, at: &Path) {
        Self::parents(at);
        std::os::unix::fs::symlink(target, at).expect("symlink");
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn build(scenario: &Value) -> (World, FakeRunner, Paths) {
    let name = scenario["name"].as_str().unwrap_or("unnamed");
    let root =
        std::env::temp_dir().join(format!("garage-update-trace-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    let world = World {
        checkout: root.join("checkout"),
        home: root.join("home"),
        plugins: root.join("plugins"),
        root,
    };
    std::fs::create_dir_all(&world.home).expect("home");
    build_checkout(&world, scenario);
    build_home(&world, scenario);
    let env: HashMap<String, String> =
        [("HOME".to_owned(), world.home.to_string_lossy().into_owned())]
            .into_iter()
            .collect();
    let mut paths = Paths::from_env_map(&env);
    paths.plugin_root = world.plugins.clone();
    (world, runner_for(scenario), paths)
}

/// The scratch checkout: the stow package, `.git` when this case has one, `bootstrap.sh`
/// unless this case is the one that is missing it, and the pins file.
fn build_checkout(world: &World, scenario: &Value) {
    let ignore = scenario["stow_ignore"].as_str().unwrap_or("");
    let mut trees = vec![world.checkout.clone()];
    if scenario["other_checkout"].as_bool().unwrap_or(false) {
        trees.push(world.root.join("other-checkout"));
    }
    for tree in trees {
        let desktop = tree.join("desktop");
        std::fs::create_dir_all(&desktop).expect("desktop");
        std::fs::write(desktop.join(".stow-local-ignore"), ignore).expect("ignore");
        world.plant(&desktop, &scenario["tree"]);
    }
    if scenario["has_git"].as_bool().unwrap_or(true) {
        std::fs::create_dir_all(world.checkout.join(".git")).expect("git dir");
    }
    if !scenario["no_bootstrap"].as_bool().unwrap_or(false) {
        std::fs::write(world.checkout.join("bootstrap.sh"), "#!/bin/sh\nexit 0\n")
            .expect("bootstrap");
    }
    let pins = scenario["pins"].as_str().unwrap_or("");
    if !pins.is_empty() {
        std::fs::create_dir_all(world.checkout.join("system")).expect("system dir");
        std::fs::write(world.checkout.join("system/plugin-pins"), pins).expect("pins");
    }
}

/// The scratch `$HOME`: the managed links (into this checkout or another one), the links
/// the sweep is meant to find, and whatever is deployed under the plugin root.
fn build_home(world: &World, scenario: &Value) {
    let source = if scenario["links"].as_str() == Some("other") {
        "$OTHER"
    } else {
        "$CHECKOUT"
    };
    for relative in strings(scenario.get("managed")) {
        let target = world.expand(&format!("{source}/desktop/{relative}"));
        World::link(&target, &world.home.join(&relative));
    }
    for (relative, target) in scenario["dangling"].as_object().into_iter().flatten() {
        let where_ = world.expand(target.as_str().unwrap_or(""));
        World::link(&where_, &world.home.join(relative));
    }
    world.plant(&world.plugins, &scenario["plugins"]);
}

/// The fake runner this scenario answers through.
fn runner_for(scenario: &Value) -> FakeRunner {
    let mut git = HashMap::new();
    for (key, pair) in scenario["git"].as_object().into_iter().flatten() {
        let status = pair
            .get(0)
            .and_then(Value::as_i64)
            .and_then(|status| i32::try_from(status).ok())
            .unwrap_or(0);
        let stdout = pair.get(1).and_then(Value::as_str).unwrap_or("").to_owned();
        git.insert(key.clone(), (status, stdout));
    }
    FakeRunner {
        git,
        heads: RefCell::new(
            scenario["heads"]
                .as_array()
                .map(|heads| heads.iter().map(output_pair).collect())
                .unwrap_or_default(),
        ),
        hyprland: scenario["hyprland"].as_str().unwrap_or("").to_owned(),
        compositor: number(scenario, "compositor"),
        bootstrap_status: number(scenario, "bootstrap_status"),
        trace: RefCell::new(Vec::new()),
        streamed: RefCell::new(Vec::new()),
    }
}

fn output_pair(pair: &Value) -> (i32, String) {
    let status = pair
        .get(0)
        .and_then(Value::as_i64)
        .and_then(|status| i32::try_from(status).ok())
        .unwrap_or(0);
    let stdout = pair.get(1).and_then(Value::as_str).unwrap_or("").to_owned();
    (status, stdout)
}

/// One `i32` out of the scenario, defaulting to zero.
fn number(scenario: &Value, key: &str) -> i32 {
    scenario[key]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0)
}

fn space_for(scenario: &Value) -> FixedSpace {
    FixedSpace::gib(scenario["space_gib"].as_u64().unwrap_or(6))
}

/// One at a time, and not in parallel with anything else in this module: the two
/// environment variables `update` sets around the streamed call are process-global, so two
/// of these running at once would read each other's.
#[test]
fn every_scenario_prints_and_runs_what_the_python_did() {
    let _environment = ENVIRONMENT.lock().expect("update test environment");
    let document: Value =
        serde_json::from_str(TRACES).expect("testdata/update_traces.json is valid JSON");
    let scenarios = document
        .get("scenarios")
        .and_then(Value::as_array)
        .expect("a list of scenarios");
    assert!(scenarios.len() >= 12, "the corpus lost scenarios");
    for scenario in scenarios {
        check(scenario);
    }
}

/// One scenario: the transcript, the status or the refusal, the captured trace, and the
/// streamed delegation.
fn check(scenario: &Value) {
    let name = scenario["name"].as_str().unwrap_or("unnamed");
    let (world, runner, paths) = build(scenario);
    let _held = scenario["lock_started"].as_str().map(|started| {
        UpdateLock::acquire_with_started(&paths.locks.update, started)
            .expect("the scenario's update lock is free")
    });
    let argv = strings(scenario.get("argv"));
    let mut out = String::new();
    let outcome = update_at(
        &paths,
        &runner,
        &argv,
        UpdateRun {
            root: Some(world.checkout.clone()),
            captured: Some(&mut out),
            space: &space_for(scenario),
        },
    );
    if argv.iter().any(|argument| argument == "--dry-run") {
        assert!(
            !paths.state_root.join("updates").exists(),
            "{name}: a dry run created the transcript directory"
        );
    }
    assert_eq!(
        world.normalize(&out),
        scenario["stdout"].as_str().unwrap_or(""),
        "{name}: the transcript"
    );
    match scenario["status"].as_i64() {
        Some(status) => assert_eq!(
            i64::from(outcome.expect("the run finished")),
            status,
            "{name}: the exit status"
        ),
        None => assert_eq!(
            outcome
                .err()
                .map(|error| world.normalize(&error.to_string()))
                .as_deref(),
            scenario["error"].as_str(),
            "{name}: the refusal"
        ),
    }
    check_calls(&runner, &world, scenario, name);
    drop(std::fs::remove_dir_all(&world.root));
}

/// The cheap live-path substitute: a real (non-dry) `run_steps` over a scratch home, with
/// every process boundary faked. It proves the parent report and the private file are the
/// same bytes while the fake pull moves between two distinct commits.
#[test]
fn a_real_scratch_run_persists_the_header_and_complete_parent_report() {
    let _environment = ENVIRONMENT.lock().expect("update test environment");
    let scenario = real_transcript_scenario();
    let (world, runner, paths) = build(&scenario);
    let cx = DoctorCx::at(&paths, &runner, world.checkout.clone());
    let mut report = Report::new(false, true);

    let status = run_steps(&cx, &mut report, false, &space_for(&scenario)).expect("scratch update");
    assert_eq!(status, 0);
    let captured = report.captured().expect("captured report");
    assert_real_transcript(captured, &paths);
    drop(report);
    drop(std::fs::remove_dir_all(&world.root));
}

fn real_transcript_scenario() -> Value {
    serde_json::json!({
        "name": "real-transcript",
        "tree": {},
        "has_git": true,
        "git": {
            "rev-parse --abbrev-ref --symbolic-full-name @{upstream}": [0, "origin/main"],
            "fetch --quiet": [0, ""],
            "log --oneline HEAD..origin/main": [0, "2222222 transcript feature"],
            "status --porcelain": [0, ""],
            "merge --ff-only origin/main": [0, ""],
            "rev-parse --short HEAD": [0, "2222222"]
        },
        "heads": [[0, "1111111111111111111111111111111111111111"],
                  [0, "2222222222222222222222222222222222222222"]],
        "hyprland": "",
        "compositor": 1,
        "bootstrap_status": 0
    })
}

fn assert_real_transcript(captured: &str, paths: &Paths) {
    let expected_header = format!(
        concat!(
            "Garage update\n",
            "    checkout commit before pull  1111111111111111111111111111111111111111\n",
            "    checkout commit after pull   2222222222222222222222222222222222222222\n",
            "    binary                       {}\n"
        ),
        binary_build()
    );
    assert!(captured.starts_with(&expected_header), "{captured}");
    assert!(captured.contains("bootstrap argv    "));
    assert!(captured.contains("bootstrap cwd     "));
    assert!(captured.contains("bootstrap env     GARAGE_SKIP_PLUGIN_DEPLOY=1"));
    assert!(captured.contains("bootstrap env     GARAGE_FORCE=<unset>"));
    assert!(captured.contains("bootstrap output  terminal (not included in this transcript)"));
    assert!(captured.contains("bootstrap exit    0"));

    let updates = paths.state_root.join("updates");
    let transcripts: Vec<PathBuf> = std::fs::read_dir(updates)
        .expect("updates directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    assert_eq!(transcripts.len(), 1);
    let on_disk = std::fs::read_to_string(transcripts.first().expect("one transcript"))
        .expect("transcript text");
    assert_eq!(on_disk, captured);
}

/// The two process-boundary surfaces: what was captured, and what was streamed.
fn check_calls(runner: &FakeRunner, world: &World, scenario: &Value, name: &str) {
    assert_eq!(
        runner
            .trace
            .borrow()
            .iter()
            .map(|line| world.normalize(line))
            .collect::<Vec<_>>(),
        strings(scenario.get("trace")),
        "{name}: what it asked the machine"
    );
    assert_eq!(
        streamed_of(runner, world),
        expected_streamed(scenario),
        "{name}: the streamed delegation"
    );
    // The two variables are taken back off, so the plugin rebuild that runs later in the
    // same process sees the environment the Python's own child sees.
    assert!(
        std::env::var_os("GARAGE_SKIP_PLUGIN_DEPLOY").is_none(),
        "{name}"
    );
    assert!(std::env::var_os("GARAGE_FORCE").is_none(), "{name}");
}

/// What was handed the terminal, with the scratch paths put back to their tokens.
fn streamed_of(runner: &FakeRunner, world: &World) -> Vec<Streamed> {
    runner
        .streamed
        .borrow()
        .iter()
        .map(|call| Streamed {
            command: call
                .command
                .iter()
                .map(|part| world.normalize(part))
                .collect(),
            cwd: world.normalize(&call.cwd),
            skip_plugin_deploy: call.skip_plugin_deploy.clone(),
            force: call.force.clone(),
        })
        .collect()
}

/// The same, as the fixture wrote it down.
fn expected_streamed(scenario: &Value) -> Vec<Streamed> {
    scenario["streamed"]
        .as_array()
        .expect("streamed")
        .iter()
        .map(|call| Streamed {
            command: strings(call.get("command")),
            cwd: call["cwd"].as_str().unwrap_or("").to_owned(),
            skip_plugin_deploy: call["skip_plugin_deploy"].as_str().unwrap_or("").to_owned(),
            force: call["force"].as_str().unwrap_or("").to_owned(),
        })
        .collect()
}
