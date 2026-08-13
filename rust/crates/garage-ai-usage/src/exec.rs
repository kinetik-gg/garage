//! The one place in this crate allowed to touch `std::process::Command` directly.
//!
//! `rust/clippy.toml` disallows `Command::new` workspace-wide with a reason that points at
//! `garage_proc::run`: every settings-backend crate spawns through that single process
//! boundary. This crate is not part of the settings backend -- it is a standalone,
//! once-per-invocation CLI that shells out to tokscale directly, exactly as the Python
//! script it replaces did with `subprocess.run`. [`run`] is the single function that
//! carries the `#[expect]` for that lint, matching the precedent already set in
//! `garage-waybar`'s own `exec.rs` for the same reason.
//!
//! Mirrors `subprocess.run(capture_output=True, text=True, timeout=..., check=False)`: a
//! spawn failure or the timeout expiring is the only way this returns [`RunError`],
//! matching the pair of exceptions (`OSError`, `subprocess.SubprocessError`) the Python's
//! own `except` catches. A process that runs and exits non-zero is `Ok` with a non-zero
//! [`Output::status`], exactly as `check=False` leaves it -- neither Python call site here
//! ever reads `result.returncode`. stderr is discarded at the source (`Stdio::null()`)
//! rather than captured and dropped, for the same reason: `result.stderr` is never read
//! either, in `load_usage()` or in `load_today()`.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a finished process left behind: the exit code and whatever it wrote to stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Output {
    pub(crate) status: i32,
    pub(crate) stdout: String,
}

/// Why [`run`] could not produce an [`Output`] at all.
#[derive(Debug)]
pub(crate) enum RunError {
    /// The binary could not be started, or waiting on it failed outright -- Python's
    /// `OSError` (a missing executable is `FileNotFoundError`, a subclass).
    Spawn(std::io::Error),
    /// The command did not finish inside `timeout` and was killed -- Python's
    /// `subprocess.TimeoutExpired`, a `subprocess.SubprocessError` subclass.
    Timeout,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(source) => write!(f, "could not run the command: {source}"),
            Self::Timeout => write!(f, "the command did not finish in time"),
        }
    }
}

impl std::error::Error for RunError {}

/// Run `argv[0]` with the rest of `argv` as arguments, waiting up to `timeout`.
///
/// stdout is read on a background thread while the main thread polls for exit, so a child
/// that writes more than one pipe buffer's worth of output before exiting cannot deadlock
/// this call the way a naive "wait, then read" would.
#[expect(
    clippy::disallowed_methods,
    reason = "the AI usage widget is not the settings backend; it shells out to tokscale \
              directly, exactly as the Python script it replaces did with subprocess.run"
)]
pub(crate) fn run(argv: &[&str], timeout: Duration) -> Result<Output, RunError> {
    let (program, rest) = argv
        .split_first()
        .ok_or_else(|| RunError::Spawn(std::io::Error::other("empty command")))?;
    let mut child = Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(RunError::Spawn)?;

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

/// Read a child's stdout to completion on its own thread, so the caller can poll
/// `try_wait` without risking a full pipe buffer blocking the child forever.
fn spawn_stdout_reader(mut pipe: std::process::ChildStdout) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = pipe.read_to_string(&mut buffer);
        buffer
    })
}

/// Poll `child` until it exits or `timeout` elapses, killing and reaping it on timeout --
/// matching `subprocess.run`'s own behaviour when its `timeout` expires.
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
        let output = run(&["/bin/echo", "-n", "hello"], Duration::from_secs(2))
            .expect("echo is always on PATH in this test environment");
        assert_eq!(0, output.status);
        assert_eq!("hello", output.stdout);
    }

    #[test]
    fn a_non_zero_exit_is_ok_with_the_status_set() {
        let output = run(
            &["/bin/sh", "-c", "echo partial; exit 7"],
            Duration::from_secs(2),
        )
        .expect("sh is always on PATH in this test environment");
        assert_eq!(7, output.status);
        assert_eq!("partial\n", output.stdout);
    }

    #[test]
    fn a_missing_binary_is_a_spawn_error() {
        let result = run(
            &["/no/such/binary-garage-ai-usage-test"],
            Duration::from_secs(2),
        );
        assert!(matches!(result, Err(RunError::Spawn(_))));
    }

    #[test]
    fn a_slow_command_is_killed_and_reported_as_a_timeout() {
        let started = std::time::Instant::now();
        let result = run(&["/bin/sleep", "30"], Duration::from_millis(50));
        assert!(matches!(result, Err(RunError::Timeout)));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the timeout, not the sleep, must be what ends the wait"
        );
    }

    #[test]
    fn output_larger_than_one_pipe_buffer_does_not_deadlock() {
        let output = run(
            &["/bin/sh", "-c", "yes | head -c 200000"],
            Duration::from_secs(5),
        )
        .expect("sh/yes/head are always on PATH in this test environment");
        assert_eq!(0, output.status);
        assert_eq!(200_000, output.stdout.len());
    }
}
