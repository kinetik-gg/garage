//! The one place in this crate allowed to touch `std::process::Command` directly.
//!
//! `rust/clippy.toml` disallows `Command::new` workspace-wide with a reason that points
//! at `garage_proc::run`: every other crate spawns through the settings backend's single
//! process boundary, which owns the timeout, the capture and the differential trace.
//! `garage-metrics` cannot use it, for the same reason `garage-waybar` cannot -- this
//! binary runs once per Waybar tick, independent of the settings backend, exactly as the
//! Python script it replaces shelled out on its own. [`spawn`] is the single function
//! that carries the `#[expect]` for that lint, so the exception is visible in one place.
//!
//! Two commands reach it, and only two. `findmnt` names the partition backing `/`, once
//! per session, because sysfs has no path from a mount point to a block device.
//! `nvidia-smi` is the only interface the proprietary driver offers at all -- there is
//! no busy or VRAM attribute anywhere under `/sys/class/drm/cardN/device` for it. Every
//! other reading in this binary is a file.
//!
//! [`run`] is `subprocess.run(command, text=True, capture_output=True, timeout=2)`
//! followed by the Python helper's own two rules: a spawn failure or a timeout is
//! `None`, and so is a non-zero exit. Folding those three into one `None` is what makes
//! a machine with no `nvidia-smi` installed indistinguishable from one with no NVIDIA
//! card, which is the answer the caller wants in both cases.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The Python's `run(command, timeout=2)` default, and the only value either call site
/// uses.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(2);

/// The single spot that actually constructs a [`Command`].
#[expect(
    clippy::disallowed_methods,
    reason = "metrics samples findmnt and nvidia-smi directly; it is not the settings backend, and \
              runs once per waybar tick exactly as the Python script it replaces did"
)]
fn spawn(program: &str, rest: &[&str]) -> std::io::Result<std::process::Child> {
    Command::new(program)
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// Best-effort subprocess. A missing binary, a timeout or a non-zero exit is a `None`,
/// not an exception.
///
/// stderr is discarded at the source rather than captured and dropped, because the
/// Python's `run()` never reads `result.stderr` -- `nvidia-smi` on a machine with the
/// driver half-installed is chatty there, and none of it has anywhere to go.
///
/// stdout is drained on its own thread while the main thread polls for exit, so a child
/// that fills a pipe buffer before exiting cannot deadlock a Waybar tick. `nvidia-smi`
/// on a four-GPU box is nowhere near 64KiB, but a hang here is a bar module that stops
/// updating, and the thread costs nothing.
pub(crate) fn run(command: &[&str]) -> Option<String> {
    let (program, rest) = command.split_first()?;
    let mut child = spawn(program, rest).ok()?;
    let reader = child.stdout.take().map(drain);
    let status = wait(&mut child)?;
    let stdout = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    if status == 0 {
        return Some(stdout);
    }
    None
}

fn drain(mut pipe: std::process::ChildStdout) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = pipe.read_to_string(&mut buffer);
        buffer
    })
}

/// Poll until the child exits or the timeout elapses, killing and reaping it in the
/// second case -- what `subprocess.run`'s own `timeout` does. A timeout is `None`,
/// which is where this differs from `garage-waybar`'s copy: the Python's `run()` helper
/// here catches `subprocess.SubprocessError` itself rather than letting the caller see
/// it, so a timed-out `nvidia-smi` is a machine with no GPU for one tick.
fn wait(child: &mut std::process::Child) -> Option<i32> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.code().unwrap_or(-1)),
            Ok(None) => (),
            Err(_) => return None,
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn a_successful_command_gives_back_its_stdout() {
        assert_eq!(
            run(&["/bin/echo", "nvme0n1p2"]).as_deref(),
            Some("nvme0n1p2\n")
        );
    }

    #[test]
    fn a_non_zero_exit_is_none_even_when_it_printed_something() {
        assert_eq!(run(&["/bin/sh", "-c", "echo out; exit 1"]), None);
    }

    #[test]
    fn a_missing_binary_is_none_rather_than_a_panic() {
        assert_eq!(run(&["/no/such/garage-metrics-binary"]), None);
    }

    #[test]
    fn an_empty_command_is_none() {
        assert_eq!(run(&[]), None);
    }

    #[test]
    fn output_larger_than_a_pipe_buffer_does_not_deadlock() {
        let output = run(&["/bin/sh", "-c", "yes | head -c 200000"]).expect("sh runs");
        assert_eq!(output.len(), 200_000);
    }
}
