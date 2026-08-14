//! Both of `doctor`'s output modes, byte for byte, against the Python's own.
//!
//! `testdata/doctor_fixtures.json` is the output of a throwaway generator (not committed --
//! the same arrangement `keybind::parity` and `workspace_trace_tests` use). It loads
//! `desktop/.local/bin/garage` through `SourceFileLoader`, builds one scratch world per
//! scenario, drives `doctor([])` and `doctor(["--report"])` against a fake `run()`, and writes
//! down both transcripts, both exit statuses, both traces and the world that produced them.
//! Nothing here is transcribed by hand, so nothing here can be transcribed wrong.
//!
//! # Why the scratch checkout carries its own manifests
//!
//! The generator writes `system/manifest/{packages,units,fonts}.list` *from the Python's own
//! `DOCTOR_PACKAGES` / `DOCTOR_UNITS` / `DOCTOR_FONTS`*. That is what makes the comparison a
//! comparison rather than a diff of two different package sets: the Rust doctor reads those
//! files at runtime (see [`super`]'s module doc), so handing it the Python's compiled-in
//! tuples asks both implementations the same question. It is also the proof that the runtime
//! reading works -- every one of these scenarios answers from a file that did not exist when
//! this crate was compiled.
//!
//! # What is compared
//!
//! stdout for both modes, the exit status for both modes, and the trace: every command either
//! implementation ran, in order, with its arguments. The trace is the surface that catches a
//! port which prints the right answer without asking the machine -- or which asks it twice.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use garage_core::paths::Paths;
use garage_core::traits::{Output, RunError, Runner};
use serde_json::Value;

use super::{report, transcript, DoctorCx, Installed};

const FIXTURES: &str = include_str!("../../testdata/doctor_fixtures.json");

/// Answers from the scenario's table, and remembers what it was asked.
struct FakeRunner {
    /// Joined argv -> (status, stdout). `git` is keyed without its `-C <root>`, which is the
    /// only argument that carries a scratch path.
    answers: HashMap<String, (i32, String)>,
    /// Fragment file names `luac -p` refuses.
    luac_bad: Vec<String>,
    /// Every command, joined, in the order it was run.
    trace: RefCell<Vec<String>>,
}

impl Runner for FakeRunner {
    fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
        let joined = command.join(" ");
        self.trace.borrow_mut().push(joined.clone());
        if command.first() == Some(&"luac") && command.get(1) == Some(&"-p") {
            let name = command
                .get(2)
                .and_then(|path| Path::new(path).file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let bad = self.luac_bad.contains(&name);
            return Ok(Output {
                status: i32::from(bad),
                stdout: String::new(),
                stderr: String::new(),
            });
        }
        let key = if command.first() == Some(&"git") {
            let rest = command.get(3..).unwrap_or_default().join(" ");
            format!("git {rest}")
        } else {
            joined
        };
        let (status, stdout) = self
            .answers
            .get(&key)
            .cloned()
            .unwrap_or((0, String::new()));
        Ok(Output {
            status,
            stdout,
            stderr: String::new(),
        })
    }

    fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
        unreachable!("doctor never detaches a child")
    }

    fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
        unreachable!("doctor never hands over the terminal")
    }
}

/// A scratch world, and the four places the fixtures' `$TOKEN`s stand for.
struct World {
    root: PathBuf,
    checkout: PathBuf,
    home: PathBuf,
    plugins: PathBuf,
}

impl World {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "garage-doctor-parity-{name}-{}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&root));
        let world = Self {
            checkout: root.join("checkout"),
            home: root.join("home"),
            plugins: root.join("plugins"),
            root,
        };
        fs::create_dir_all(&world.home).expect("scratch home");
        world
    }

    fn other(&self) -> PathBuf {
        self.root.join("other-checkout")
    }

    /// `$CHECKOUT`, `$OTHER`, `$HOME` and `$PLUGINS` filled in.
    fn expand(&self, text: &str) -> String {
        text.replace("$CHECKOUT", &self.checkout.to_string_lossy())
            .replace("$OTHER", &self.other().to_string_lossy())
            .replace("$HOME", &self.home.to_string_lossy())
            .replace("$PLUGINS", &self.plugins.to_string_lossy())
    }

    /// The inverse, for comparing against what the Python printed.
    fn normalize(&self, text: &str) -> String {
        text.replace(&self.checkout.to_string_lossy().into_owned(), "$CHECKOUT")
            .replace(&self.other().to_string_lossy().into_owned(), "$OTHER")
            .replace(&self.home.to_string_lossy().into_owned(), "$HOME")
            .replace(&self.plugins.to_string_lossy().into_owned(), "$PLUGINS")
    }

    /// One `{relpath: contents | ["symlink", target]}` table, planted under `base`.
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
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        let link = content
            .as_array()
            .filter(|pair| pair.first() == Some(&Value::String("symlink".to_owned())))
            .and_then(|pair| pair.get(1))
            .and_then(Value::as_str);
        match link {
            Some(where_) => {
                std::os::unix::fs::symlink(self.expand(where_), target).expect("symlink");
            }
            None => {
                fs::write(target, content.as_str().unwrap_or_default()).expect("write");
            }
        }
    }
}

/// The shipped defaults, reached the way a stowed machine reaches them: a symlink into the
/// repository, which is also what this module's scratch homes plant.
fn link_defaults(home: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../desktop/.config/garage/preferences.defaults.toml");
    let link = home.join(".config/garage/preferences.defaults.toml");
    fs::create_dir_all(link.parent().unwrap_or(home)).expect("config dir");
    if !link.exists() {
        std::os::unix::fs::symlink(source, link).expect("defaults symlink");
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
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

/// `{joined argv: [status, stdout]}` as the fake runner wants it.
fn answers_of(table: &Value) -> HashMap<String, (i32, String)> {
    let mut answers = HashMap::new();
    for (key, pair) in table.as_object().into_iter().flatten() {
        let status = pair
            .get(0)
            .and_then(Value::as_i64)
            .and_then(|status| i32::try_from(status).ok())
            .unwrap_or(0);
        let stdout = pair.get(1).and_then(Value::as_str).unwrap_or("").to_owned();
        answers.insert(key.clone(), (status, stdout));
    }
    answers
}

/// The scratch checkout: the stow package, its `.stow-local-ignore`, the manifests and the
/// pins file.
fn build_checkout(world: &World, document: &Value, scenario: &Value) {
    let ignore = document["stow_ignore"].as_str().unwrap_or("");
    let mut trees = vec![world.checkout.clone()];
    if scenario["other_checkout"].as_bool().unwrap_or(false) {
        trees.push(world.other());
    }
    for tree in trees {
        let desktop = tree.join("desktop");
        fs::create_dir_all(&desktop).expect("desktop");
        fs::write(desktop.join(".stow-local-ignore"), ignore).expect("ignore file");
        world.plant(&desktop, &scenario["tree"]);
    }
    let manifest = world.checkout.join("system/manifest");
    fs::create_dir_all(&manifest).expect("manifest dir");
    world.plant(&manifest, &scenario["manifest"]);
    let pins = scenario["pins"].as_str().unwrap_or("");
    if !pins.is_empty() {
        fs::write(world.checkout.join("system/plugin-pins"), pins).expect("pins");
    }
}

/// The scratch `$HOME`: the managed links, the deployed plugins, the rendered fragments and
/// whatever `preferences.toml` this scenario starts from.
fn build_home(world: &World, scenario: &Value) {
    world.plant(&world.home, &scenario["managed_home"]);
    world.plant(&world.plugins, &scenario["plugins"]);
    world.plant(
        &world.home.join(".local/state/garage/generated"),
        &scenario["fragments"],
    );
    link_defaults(&world.home);
    let preferences = scenario["preferences"].as_str().unwrap_or("");
    if !preferences.is_empty() {
        fs::write(
            world.home.join(".config/garage/preferences.toml"),
            preferences,
        )
        .expect("preferences");
    }
}

/// Rebuild one scenario's world and run both output modes against it.
fn replay(document: &Value, scenario: &Value) -> (World, FakeRunner, Paths) {
    let name = scenario["name"].as_str().unwrap_or("unnamed");
    let world = World::new(name);
    build_checkout(&world, document, scenario);
    build_home(&world, scenario);
    let runner = FakeRunner {
        answers: answers_of(&scenario["answers"]),
        luac_bad: string_list(scenario.get("luac_bad")),
        trace: RefCell::new(Vec::new()),
    };
    let env: HashMap<String, String> =
        [("HOME".to_owned(), world.home.to_string_lossy().into_owned())]
            .into_iter()
            .collect();
    let mut paths = Paths::from_env_map(&env);
    paths.plugin_root = world.plugins.clone();
    (world, runner, paths)
}

#[test]
fn every_scenario_prints_what_the_python_printed_and_runs_what_it_ran() {
    let document: Value =
        serde_json::from_str(FIXTURES).expect("testdata/doctor_fixtures.json is valid JSON");
    let scenarios = document
        .get("scenarios")
        .and_then(Value::as_array)
        .expect("scenarios is a list")
        .clone();
    assert!(scenarios.len() >= 20, "the corpus lost scenarios");
    for scenario in &scenarios {
        check(&document, scenario);
    }
}

/// One scenario, both output modes.
fn check(document: &Value, scenario: &Value) {
    let name = scenario["name"].as_str().unwrap_or("unnamed");
    let (world, runner, paths) = replay(document, scenario);
    let mut cx = DoctorCx::at(&paths, &runner, world.checkout.clone());
    cx.installed = Installed::Named(string_list(scenario.get("binaries")));

    let (text, status) = transcript(&cx);
    assert_eq!(
        world.normalize(&text),
        scenario["lines_stdout"].as_str().unwrap_or(""),
        "{name}: the printed report"
    );
    assert_eq!(
        i64::from(status),
        scenario["lines_status"].as_i64().unwrap_or(0),
        "{name}: the printed report's exit status"
    );
    assert_eq!(
        traced(&runner, &world),
        string_list(scenario.get("lines_trace")),
        "{name}: what the printed report asked the machine"
    );

    runner.trace.borrow_mut().clear();
    let (text, status) = report::report_text(&cx);
    assert_eq!(
        blank_the_clock(&world.normalize(&text)),
        scenario["report_stdout"].as_str().unwrap_or(""),
        "{name}: --report"
    );
    assert_eq!(
        i64::from(status),
        scenario["report_status"].as_i64().unwrap_or(0),
        "{name}: --report's exit status"
    );
    assert_eq!(
        traced(&runner, &world),
        string_list(scenario.get("report_trace")),
        "{name}: what --report asked the machine"
    );
    drop(fs::remove_dir_all(&world.root));
}

/// What the runner was asked, with the scratch paths put back to their tokens.
fn traced(runner: &FakeRunner, world: &World) -> Vec<String> {
    runner
        .trace
        .borrow()
        .iter()
        .map(|line| world.normalize(line))
        .collect()
}

/// `generated_at` is the one field that cannot agree between two runs, let alone two
/// implementations: it is the wall clock. Its *shape* is asserted separately, by
/// [`the_clock_field_is_iso_8601_with_an_offset`].
fn blank_the_clock(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.trim_start().starts_with("\"generated_at\":") {
                "  \"generated_at\": \"$NOW\","
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// The `generated_at` timestamp contract, applied here:
/// `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[+-]\d{4}$`.
#[test]
fn the_clock_field_is_iso_8601_with_an_offset() {
    let stamp = super::local_iso8601(super::now_seconds());
    let bytes: Vec<char> = stamp.chars().collect();
    assert_eq!(bytes.len(), 24, "{stamp}");
    let digits = [
        0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22, 23,
    ];
    for index in digits {
        assert!(
            bytes.get(index).is_some_and(char::is_ascii_digit),
            "{stamp} position {index}"
        );
    }
    assert_eq!(bytes.get(4), Some(&'-'));
    assert_eq!(bytes.get(7), Some(&'-'));
    assert_eq!(bytes.get(10), Some(&'T'));
    assert_eq!(bytes.get(13), Some(&':'));
    assert_eq!(bytes.get(16), Some(&':'));
    assert!(matches!(bytes.get(19), Some(&'+' | &'-')), "{stamp}");
}
