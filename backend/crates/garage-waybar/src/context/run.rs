//! `context-status.py`'s own `run(command)`: `subprocess.run(...).stdout`, with no
//! check of the exit code at all -- deliberately simpler than `media-status.py`'s
//! `run()`, which folds a non-zero exit into `""`. `containers()`, `microphone()` and
//! `smb()` all read whatever came out on stdout regardless of how the command exited;
//! only a spawn failure or the shared 3-second timeout is a real error here.

use std::time::Duration;

use crate::exec::{self, RunError};

pub(crate) fn run_ignoring_status(argv: &[&str], timeout: Duration) -> Result<String, RunError> {
    Ok(exec::run(argv, timeout)?.stdout)
}
