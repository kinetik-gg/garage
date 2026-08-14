//! One scripted fake machine, shared by every applier's own tests.
//!
//! The appliers in this crate are almost entirely *trace*: what `push_theme()` does is issue
//! eight commands in one order, and what `apply_file_index()` does is issue two or three
//! depending on a preference. A test for one of them is therefore a test about a recorded
//! argv list, and every one of them needs the same three things -- a scratch `HOME` with the
//! real [`Paths`] shape over it, a [`Preferences`] built from a few departures, and a
//! [`Runner`] that records what it was asked and answers from a table.
//!
//! Written once here rather than per file for the reason the crate's own trace-fixture
//! modules already show: three copies of a recording `Runner` had appeared before this
//! module existed, and a fourth would have been the point at which they started to disagree
//! about what "the command could not be run at all" looks like.
//!
//! The recorder backs [`Hyprctl`] as well as the runner, which is what production does, so a
//! `hyprctl monitors -j` the *render* half asks for lands in the same trace as the
//! `hyprctl reload` the apply half issues. That matters: half the parity questions in this
//! crate are about interleaving, and a harness that hid the render's own reads would answer
//! them wrongly.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use garage_core::paths::Paths;
use garage_core::schema::defaults::Defaults;
use garage_core::schema::notes::Notes;
use garage_core::schema::Preferences;
use garage_core::traits::{LuaCheckError, LuaSyntaxCheck, Output, RunError, Runner};
use garage_proc::Hyprctl;
use garage_render::cx::RenderCx;

use crate::cx::SessionCx;

static SERIAL: AtomicU64 = AtomicU64::new(0);

/// What the fake machine says when a given command is run.
enum Answer {
    /// The process started and exited: status, stdout, stderr.
    Ran(i32, String, String),
    /// The process could not be started at all -- the `OSError` half of the Python's `run()`.
    Refused(String),
}

/// The scripted answers, keyed by the joined argv.
///
/// Exact match first, then the longest key the joined argv starts with -- the same two-step
/// lookup used by the recorded trace fixtures.
#[derive(Default)]
pub(crate) struct Script {
    answers: Vec<(String, Answer)>,
}

impl Script {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The command ran and said this.
    pub(crate) fn answering(
        mut self,
        command: &str,
        status: i32,
        stdout: &str,
        stderr: &str,
    ) -> Self {
        self.answers.push((
            command.to_owned(),
            Answer::Ran(status, stdout.to_owned(), stderr.to_owned()),
        ));
        self
    }

    /// The command exited non-zero with nothing to say -- the common shape.
    pub(crate) fn failing(self, command: &str) -> Self {
        self.answering(command, 1, "", "")
    }

    /// The command could not be started at all.
    pub(crate) fn refusing(mut self, command: &str, detail: &str) -> Self {
        self.answers
            .push((command.to_owned(), Answer::Refused(detail.to_owned())));
        self
    }

    fn lookup(&self, line: &str) -> Option<&Answer> {
        if let Some((_, answer)) = self.answers.iter().find(|(key, _)| key == line) {
            return Some(answer);
        }
        self.answers
            .iter()
            .filter(|(key, _)| line.starts_with(key.as_str()))
            .max_by_key(|(key, _)| key.len())
            .map(|(_, answer)| answer)
    }
}

/// The [`Runner`] the scripted machine hands out: records every call, answers from the table.
pub(crate) struct Recorder {
    script: Script,
    calls: RefCell<Vec<String>>,
}

impl Runner for Recorder {
    fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
        let line = command.join(" ");
        self.calls.borrow_mut().push(line.clone());
        match self.script.lookup(&line) {
            Some(Answer::Refused(detail)) => Err(RunError {
                detail: detail.clone(),
            }),
            Some(Answer::Ran(status, stdout, stderr)) => Ok(Output {
                status: *status,
                stdout: stdout.clone(),
                stderr: stderr.clone(),
            }),
            // "The command exists and had nothing to say", which is what most signalling
            // calls get and the scripted runner's own default.
            None => Ok(Output {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }),
        }
    }

    fn spawn_detached(&self, command: &[&str]) -> Result<(), RunError> {
        // Prefixed, because a detached spawn is a different observable act from a run: the
        // Python uses `subprocess.Popen(..., start_new_session=True)` for exactly two calls
        // and a test that could not tell them apart would not be testing the distinction.
        self.calls
            .borrow_mut()
            .push(format!("spawn: {}", command.join(" ")));
        Ok(())
    }

    fn run_streamed(&self, command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
        self.calls
            .borrow_mut()
            .push(format!("stream: {}", command.join(" ")));
        Ok(0)
    }
}

/// A Lua check that accepts everything: `luac -p` is elided from these traces, because the
/// Rust side reaches Lua through [`LuaSyntaxCheck`] rather than the runner and the candidate
/// path it is handed is a fresh temporary name on every run.
struct LuaAccepts;

impl LuaSyntaxCheck for LuaAccepts {
    fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
        Ok(())
    }
}

/// A scratch `HOME`, the preferences over it, and the scripted machine around both.
pub(crate) struct World {
    pub(crate) home: PathBuf,
    pub(crate) paths: Paths,
    pub(crate) prefs: Preferences,
    proc: Recorder,
}

impl World {
    /// Build one. `departures` is a `preferences.toml` body -- the departures alone, coerced
    /// over the shipped defaults exactly as a load would leave them.
    pub(crate) fn new(label: &str, departures: &str, script: Script) -> Self {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let home = std::env::temp_dir().join(format!(
            "garage-apply-{label}-{}-{serial}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&home));
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        let paths = Paths::from_env_map(&env);
        let table: toml::Table = departures.parse().expect("the fixture body parses");
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let mut notes = Notes::new();
        let prefs = Preferences::coerce_from(&table, &defaults, &mut notes);
        Self {
            home,
            paths,
            prefs,
            proc: Recorder {
                script,
                calls: RefCell::new(Vec::new()),
            },
        }
    }

    /// The shipped defaults with no departures at all.
    pub(crate) fn plain(label: &str, script: Script) -> Self {
        Self::new(label, "", script)
    }

    /// Run one applier against this world.
    pub(crate) fn with<R>(&self, body: impl FnOnce(&mut SessionCx<'_>) -> R) -> R {
        let monitors = Hyprctl::new(&self.proc);
        let lua = LuaAccepts;
        let render = RenderCx::new(&self.prefs, &self.paths, &monitors, &lua);
        let mut session = SessionCx::new(render, &self.proc);
        body(&mut session)
    }

    /// The runner itself, for the few entry points that take `(paths, proc)` rather than a
    /// context -- `action()` is the whole list.
    pub(crate) fn runner(&self) -> &dyn Runner {
        &self.proc
    }

    /// Every command the world was asked to run, in order.
    pub(crate) fn trace(&self) -> Vec<String> {
        self.proc.calls.borrow().clone()
    }

    /// The trace with the render half's own reads dropped, for the tests that are about what
    /// an applier *signals* rather than about what it had to ask on the way there.
    pub(crate) fn signals(&self) -> Vec<String> {
        self.trace()
            .into_iter()
            .filter(|line| !line.starts_with("hyprctl monitors"))
            .collect()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.home));
    }
}

impl std::fmt::Debug for World {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("World")
            .field("home", &self.home)
            .field("trace", &self.trace())
            .finish_non_exhaustive()
    }
}
