//! An offline machine for the dispatch tests.
//!
//! Every settings-backend command now reaches a real layer, and several of those layers
//! signal the session: `apply` reloads the compositor, `snapshot` interrogates `pactl` and
//! `gio`, `night-shift-sync` talks to hyprsunset. A dispatch test that used the real
//! [`System`](garage_proc::System) runner would run all of that against the developer's own
//! desktop -- so the runner is a parameter of every entry point in this crate, and this is
//! what the tests hand it.
//!
//! Answers nothing to everything: exit 0, empty stdout, empty stderr. That is the "command
//! exists and had nothing to say" shape, and it is enough for these tests, which are about
//! *which* command a name reaches rather than about what the command then does. The
//! behaviour of each layer is pinned by that layer's own tests and by the differential
//! corpus, both of which script real answers.

use std::cell::RefCell;
use std::path::Path;
use std::time::Duration;

use garage_core::traits::{Output, RunError, Runner};

/// A runner that records what it was asked and reaches nothing.
#[derive(Debug, Default)]
pub(crate) struct Offline {
    calls: RefCell<Vec<String>>,
}

impl Offline {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Every command this runner was handed, in order.
    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl Runner for Offline {
    fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
        self.calls.borrow_mut().push(command.join(" "));
        Ok(Output {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    fn spawn_detached(&self, command: &[&str]) -> Result<(), RunError> {
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
