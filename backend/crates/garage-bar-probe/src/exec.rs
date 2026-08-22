//! The one place in this crate allowed to touch `std::process::Command` directly.
//!
//! `clippy.toml` disallows `Command::new` workspace-wide with a reason that points at
//! `garage_proc::run`: the settings backend spawns through its single process boundary,
//! which owns capture, timeouts and the scripted test seam. This binary is not the
//! settings backend -- it polls container engines and `gio` on a timer of its own, from
//! one persistent process, which no backend runner is shaped for. [`run`] carries the
//! single `#[expect]` so the exception stays visible in one place.
//!
//! A spawn failure or an expired timeout is the only way this returns [`RunError`]; a
//! process that exits non-zero is `Ok` with its status carried, because `docker ps` on a
//! daemonless socket and `gio` on a session bus hiccup both speak through exit codes.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a finished probe left behind: the exit code and whatever it wrote to stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Output {
    pub(crate) status: i32,
    pub(crate) stdout: String,
}

/// Why `run` could not produce an [`Output`] at all.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RunError {
    /// The binary could not be started, or waiting on it failed outright.
    #[error("failed to run command: {0}")]
    Spawn(std::io::Error),
    /// The command did not finish inside its timeout and was killed.
    #[error("command timed out")]
    Timeout,
}

/// The single spot that actually constructs a [`Command`].
#[expect(
    clippy::disallowed_methods,
    reason = "the bar's context probes poll docker/gio on their own timer; they are not \
              settings-backend work and no backend runner owns their cadence"
)]
fn spawn(program: &str, rest: &[&str]) -> std::io::Result<std::process::Child> {
    Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

fn split_command<'a>(command: &'a [&'a str]) -> Result<(&'a str, &'a [&'a str]), RunError> {
    command
        .split_first()
        .map(|(program, rest)| (*program, rest))
        .ok_or_else(|| RunError::Spawn(std::io::Error::other("empty command")))
}

/// Run `command[0]` with the rest as arguments, waiting up to `timeout`, capturing stdout.
///
/// stdout is drained on a background thread while the main thread polls for exit, so a
/// child writing more than one pipe buffer cannot deadlock the call.
pub(crate) fn run(command: &[&str], timeout: Duration) -> Result<Output, RunError> {
    let (program, rest) = split_command(command)?;
    let mut child = spawn(program, rest).map_err(RunError::Spawn)?;

    let reader = child.stdout.take().map(spawn_stdout_reader);
    let status = wait_with_timeout(&mut child, timeout)?;
    let stdout = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    Ok(Output {
        status: status.code().unwrap_or(-1),
        stdout,
    })
}

fn spawn_stdout_reader(mut pipe: std::process::ChildStdout) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = pipe.read_to_string(&mut buffer);
        buffer
    })
}

fn wait_with_timeout(
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
    use super::{run, RunError};
    use std::time::Duration;

    #[test]
    fn captures_stdout_of_a_successful_command() {
        let output = run(&["/bin/echo", "hello"], Duration::from_secs(2))
            .expect("echo is always available in this environment");
        assert_eq!(0, output.status);
        assert_eq!("hello\n", output.stdout);
    }

    #[test]
    fn a_non_zero_exit_is_ok_with_the_status_set() {
        let output = run(&["/bin/sh", "-c", "exit 7"], Duration::from_secs(2))
            .expect("sh is always available in this environment");
        assert_eq!(7, output.status);
    }

    #[test]
    fn a_missing_binary_is_a_spawn_error() {
        let result = run(
            &["/no/such/binary-garage-bar-probe-test"],
            Duration::from_secs(2),
        );
        assert!(matches!(result, Err(RunError::Spawn(_))));
    }

    #[test]
    fn a_slow_command_is_killed_and_reported_as_a_timeout() {
        let result = run(&["/bin/sleep", "5"], Duration::from_millis(50));
        assert!(matches!(result, Err(RunError::Timeout)));
    }
}
