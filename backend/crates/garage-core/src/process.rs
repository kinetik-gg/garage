//! Small captured subprocesses with one timeout implementation.
//!
//! The long-running settings backend has the richer `garage-proc` boundary because it
//! needs stderr, process groups, streamed children, and a scripted test seam. The three
//! leaf data helpers only need Python's `subprocess.run(capture_output=True,
//! text=True, timeout=...)` shape: null stdin, captured stdout, discarded stderr, and a
//! child killed and reaped at the deadline. Keeping that shape here removes three copies
//! of the same polling loop without giving renderer code a route to process execution.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a finished child left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit code, or `-1` when the platform did not supply one.
    pub status: i32,
    /// Captured standard output. Standard error is discarded at the source.
    pub stdout: String,
}

/// Why a child could not produce an [`Output`].
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The command was empty, could not be spawned, or could not be waited for.
    #[error("failed to run command: {0}")]
    Spawn(std::io::Error),
    /// The child did not finish before the deadline and was killed.
    #[error("command timed out")]
    Timeout,
}

/// Run `command[0]`, capturing stdout and enforcing `timeout`.
///
/// The stdout reader runs concurrently with the wait loop so output larger than a pipe
/// buffer cannot deadlock the child before it exits. A non-zero exit is still a
/// successful run; callers decide whether that status is useful.
///
/// # Errors
///
/// [`RunError::Spawn`] when the command is empty, spawning fails, or waiting fails;
/// [`RunError::Timeout`] after killing and reaping a child that exceeded `timeout`.
pub fn run(command: &[&str], timeout: Duration) -> Result<Output, RunError> {
    let (program, rest) = command
        .split_first()
        .ok_or_else(|| RunError::Spawn(std::io::Error::other("empty command")))?;
    let mut child = spawn(program, rest).map_err(RunError::Spawn)?;
    let reader = child.stdout.take().map(read_stdout);
    let status = wait(&mut child, timeout)?;
    let stdout = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    Ok(Output {
        status: status.code().unwrap_or(-1),
        stdout,
    })
}

#[expect(
    clippy::disallowed_methods,
    reason = "this is the shared process boundary for the leaf data helpers"
)]
fn spawn(program: &str, rest: &[&str]) -> std::io::Result<std::process::Child> {
    Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

fn read_stdout(mut pipe: std::process::ChildStdout) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = pipe.read_to_string(&mut buffer);
        buffer
    })
}

fn wait(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus, RunError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(RunError::Spawn)? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{run, RunError};

    #[test]
    fn captures_stdout_and_carries_a_nonzero_status() {
        let output = run(
            &["/bin/sh", "-c", "printf partial; exit 7"],
            Duration::from_secs(2),
        )
        .expect("sh runs");
        assert_eq!(output.status, 7);
        assert_eq!(output.stdout, "partial");
    }

    #[test]
    fn empty_and_missing_commands_are_spawn_errors() {
        assert!(matches!(
            run(&[], Duration::from_secs(1)),
            Err(RunError::Spawn(_))
        ));
        assert!(matches!(
            run(&["/no/such/garage-core-process"], Duration::from_secs(1)),
            Err(RunError::Spawn(_))
        ));
    }

    #[test]
    fn a_timeout_kills_the_child() {
        assert!(matches!(
            run(&["/bin/sleep", "5"], Duration::from_millis(50)),
            Err(RunError::Timeout)
        ));
    }

    #[test]
    fn output_larger_than_a_pipe_buffer_does_not_deadlock() {
        let output = run(
            &["/bin/sh", "-c", "yes | head -c 200000"],
            Duration::from_secs(5),
        )
        .expect("sh runs");
        assert_eq!(output.stdout.len(), 200_000);
    }
}
