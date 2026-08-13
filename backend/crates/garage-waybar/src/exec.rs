//! The one place in this crate allowed to touch `std::process::Command` directly.
//!
//! `rust/clippy.toml` disallows `Command::new` workspace-wide with a reason that
//! points at `garage_proc::run`: every other crate spawns through the settings
//! backend's single process boundary, which owns the 4-second timeout, the capture and
//! the differential trace. `garage-waybar` cannot use it -- these binaries run once per
//! Waybar tick, independent of the settings backend, exactly as the Python scripts
//! they replace shelled out to `playerctl`/`hyprctl`/`pactl`/... on their own. [`run`]
//! is the single function that carries the `#[expect]` for that lint, so the exception
//! is visible in one place instead of scattered across every call site.
//!
//! Mirrors `subprocess.run(capture_output=True, text=True, timeout=..., check=False)`:
//! a spawn failure or the timeout expiring is the only way this returns [`RunError`],
//! matching the pair of exceptions (`OSError`, `subprocess.SubprocessError`) the
//! Python's own try/except catches. A process that runs and exits non-zero is `Ok`
//! with a non-zero [`Output::status`], exactly as `check=False` leaves it -- callers
//! that need Python's `run()` helpers, which fold a non-zero exit into `""`, do that
//! folding themselves.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What a finished process left behind: the exit code and whatever it wrote to
/// stdout. Neither Python `run()` helper reads stderr, so it is discarded at the
/// source (`Stdio::null()`) rather than captured and dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Output {
    pub(crate) status: i32,
    pub(crate) stdout: String,
}

/// Why `run` could not produce an [`Output`] at all.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RunError {
    /// The binary could not be started, or waiting on it failed outright -- Python's
    /// `OSError` (a missing executable is `FileNotFoundError`, a subclass).
    #[error("failed to run command: {0}")]
    Spawn(std::io::Error),
    /// The command did not finish inside `timeout` and was killed -- Python's
    /// `subprocess.TimeoutExpired`, a `subprocess.SubprocessError` subclass.
    #[error("command timed out")]
    Timeout,
}

/// The single spot that actually constructs a [`Command`]. `run` and
/// `run_inherited` are both built on this, so the `#[expect]` for
/// `clippy::disallowed_methods` lives in exactly one place even though this crate
/// spawns processes with two different stdout/stderr policies.
#[expect(
    clippy::disallowed_methods,
    reason = "waybar modules are not the settings backend; they spawn playerctl/hyprctl/pactl/etc \
              directly, exactly as the Python scripts they replace shelled out on their own"
)]
fn spawn(
    program: &str,
    rest: &[&str],
    stdout: Stdio,
    stderr: Stdio,
) -> std::io::Result<std::process::Child> {
    Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
}

fn split_command<'a>(command: &'a [&'a str]) -> Result<(&'a str, &'a [&'a str]), RunError> {
    command
        .split_first()
        .map(|(program, rest)| (*program, rest))
        .ok_or_else(|| RunError::Spawn(std::io::Error::other("empty command")))
}

/// Run `command[0]` with the rest as arguments, waiting up to `timeout`, and capture
/// its stdout. This is `subprocess.run(capture_output=True, text=True, timeout=...,
/// check=False)` -- every call in both Python scripts except the one described at
/// [`run_inherited`].
///
/// stdout is read on a background thread while the main thread polls for exit, so a
/// child that writes more than one pipe buffer's worth of output before exiting cannot
/// deadlock this call the way a naive "wait, then read" would.
pub(crate) fn run(command: &[&str], timeout: Duration) -> Result<Output, RunError> {
    let (program, rest) = split_command(command)?;
    let mut child = spawn(program, rest, Stdio::piped(), Stdio::null()).map_err(RunError::Spawn)?;

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

/// Run `command[0]` with the rest as arguments, waiting up to `timeout`, but let the
/// child inherit this process's stdout/stderr instead of capturing them.
///
/// This is `activate()`'s final `subprocess.run(["/usr/bin/hyprctl", "dispatch", ...],
/// timeout=2, check=False)` -- the one subprocess call in either Python script that
/// does **not** pass `capture_output=True`. `subprocess.run` with no
/// `stdout`/`stderr` argument inherits the parent's file descriptors, so `hyprctl`'s
/// own reply to the dispatch (typically the literal text `ok`, or an error message)
/// is printed straight through to whatever launched `--activate`, not swallowed.
/// Confirmed live: `python3 media-status.py --activate` printed `ok` to this
/// terminal on the very machine this was ported on. [`run`] would have silently
/// discarded that; this exists so the port does not.
pub(crate) fn run_inherited(command: &[&str], timeout: Duration) -> Result<i32, RunError> {
    let (program, rest) = split_command(command)?;
    let mut child =
        spawn(program, rest, Stdio::inherit(), Stdio::inherit()).map_err(RunError::Spawn)?;
    let status = wait_with_timeout(&mut child, timeout)?;
    Ok(status.code().unwrap_or(-1))
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

/// Poll `child` until it exits or `timeout` elapses; killing and reaping it on
/// timeout, matching `subprocess.run`'s own behaviour when its `timeout` expires.
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
    use super::{run, run_inherited, RunError};
    use std::time::Duration;

    #[test]
    fn captures_stdout_of_a_successful_command() {
        let output = run(&["/bin/echo", "hello", "waybar"], Duration::from_secs(2))
            .expect("echo is always on PATH in this test environment");
        assert_eq!(0, output.status);
        assert_eq!("hello waybar\n", output.stdout);
    }

    #[test]
    fn a_non_zero_exit_is_ok_with_the_status_set() {
        let output = run(&["/bin/sh", "-c", "exit 7"], Duration::from_secs(2))
            .expect("sh is always on PATH in this test environment");
        assert_eq!(7, output.status);
        assert_eq!("", output.stdout);
    }

    #[test]
    fn a_missing_binary_is_a_spawn_error() {
        let result = run(
            &["/no/such/binary-garage-waybar-test"],
            Duration::from_secs(2),
        );
        assert!(matches!(result, Err(RunError::Spawn(_))));
    }

    #[test]
    fn a_slow_command_is_killed_and_reported_as_a_timeout() {
        let result = run(&["/bin/sleep", "5"], Duration::from_millis(50));
        assert!(matches!(result, Err(RunError::Timeout)));
    }

    #[test]
    fn output_larger_than_one_pipe_buffer_does_not_deadlock() {
        // /bin/yes writes continuously; `head` caps it so the child still exits on
        // its own, but only after well over 64KiB has passed through the pipe --
        // enough to prove the background reader keeps the pipe draining.
        let output = run(
            &["/bin/sh", "-c", "yes | head -c 200000"],
            Duration::from_secs(5),
        )
        .expect("sh/yes/head are always on PATH in this test environment");
        assert_eq!(0, output.status);
        assert_eq!(200_000, output.stdout.len());
    }

    #[test]
    fn run_inherited_reports_the_exit_status_and_never_captures_anything() {
        let status = run_inherited(&["/bin/sh", "-c", "exit 3"], Duration::from_secs(2))
            .expect("sh is always on PATH in this test environment");
        assert_eq!(3, status);
    }

    #[test]
    fn run_inherited_is_also_a_spawn_error_on_a_missing_binary() {
        let result = run_inherited(
            &["/no/such/binary-garage-waybar-test"],
            Duration::from_secs(2),
        );
        assert!(matches!(result, Err(RunError::Spawn(_))));
    }
}
