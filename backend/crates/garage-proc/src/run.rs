//! `run()`, `Popen(start_new_session=True)` and the streamed call, as one type.
//!
//! The Python's `run()` is four lines and every one of them is a decision this file has to
//! reproduce: `text=True` (strings, not bytes), `capture_output=True` (both pipes read to
//! the end), `timeout` (a child that has not answered is killed rather than waited on), and
//! the `except (OSError, SubprocessError)` that turns "could not be run" into either a
//! raise or a synthesised failed run depending on `check`. `Result` is that `check` flag --
//! see [`RunError`] -- so what is left here is the capture and the clock.
//!
//! # The clock, without a timeout API
//!
//! `std::process` has no timed wait. The shape below is the standard one: both pipes are
//! drained by threads of their own, and the main thread polls
//! [`try_wait`](std::process::Child::try_wait). The threads are not an optimisation. A
//! child that fills a pipe blocks until somebody reads it, so a polling loop that also owned
//! the reading would deadlock against exactly the commands whose output is worth capturing.
//!
//! # `errno` texts, spelled Python's way
//!
//! A failure here does not stay here: `run()` with `check=False` puts `str(error)` into the
//! synthesised `CompletedProcess`'s `stderr`, and `solid_wallpaper()` raises with that
//! string. So the text is part of the contract, and [`errno_detail`] and [`timeout_detail`]
//! reproduce `OSError.__str__` and `TimeoutExpired.__str__` rather than Rust's own wording.
//!
//! # The one place `Command::new` is allowed
//!
//! `clippy.toml` bans `std::process::Command::new` across the workspace so that every spawn
//! is timed, captured and replaceable by a scripted [`Runner`] in tests. This module is the
//! exception it is banned *in favour of*, and each builder below carries the narrow allow
//! that says so.

use std::io::{self, Read, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use garage_core::pyrepr::{py_float_repr, py_str_repr};
use garage_core::traits::{Output, RunError, Runner};

/// How often the wait loop looks at the child.
///
/// Five milliseconds is far below the four-second default timeout and far above the cost of
/// a `waitpid(WNOHANG)`, so a command that answers immediately -- which is nearly all of
/// them, `hyprctl` against a local socket -- pays at most one sleep.
const POLL: Duration = Duration::from_millis(5);

/// The real machine: this is what actually forks.
///
/// A unit struct because there is nothing to configure. The environment a child inherits is
/// this process's own, matching the command boundary this replaced: callers do not pass a
/// separate environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct System;

impl Runner for System {
    fn run(&self, command: &[&str], timeout: Duration) -> Result<Output, RunError> {
        let (program, arguments) = split(command)?;
        let mut child = builder(program, arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| RunError {
                detail: errno_detail(program, &error),
            })?;
        let stdout = child.stdout.take().map(drain);
        let stderr = child.stderr.take().map(drain);
        let status = wait(&mut child, program, command, timeout)?;
        Ok(Output {
            status: exit_code(status),
            stdout: collect(stdout),
            stderr: collect(stderr),
        })
    }

    fn spawn_detached(&self, command: &[&str]) -> Result<(), RunError> {
        let (program, arguments) = split(command)?;
        // The Python passes `start_new_session=True`, which is `setsid()` in the child -- a
        // new session with no controlling terminal. `std::process` offers no safe hook that
        // runs in the child (`pre_exec` is `unsafe`, and this workspace forbids
        // `unsafe_code`), so what a *foreign* program gets here is `process_group(0)`:
        // `setpgid(0, 0)`, a new process group in the same session. The child leaves this
        // process's group, so a signal sent to that group no longer reaches it -- which is
        // the whole of what the two `timedatectl` callers need, since neither is ever
        // attached to a terminal that could hang up on it.
        //
        // The one caller that needed the session itself does not come through here: see
        // [`Runner::spawn_self_detaching`] and [`enter_new_session`].
        detached(builder(program, arguments).process_group(0), program)
    }

    fn spawn_self_detaching(&self, command: &[&str]) -> Result<(), RunError> {
        let (program, arguments) = split(command)?;
        // Deliberately no `process_group(0)`: the child inherits this process's group and is
        // therefore *not* a group leader, which is the precondition `setsid()` demands. The
        // child is this same binary and calls [`enter_new_session`] as its first act, which
        // is what actually reproduces `start_new_session=True`.
        detached(&mut builder(program, arguments), program)
    }

    fn run_streamed(&self, command: &[&str], cwd: Option<&Path>) -> Result<i32, RunError> {
        let (program, arguments) = split(command)?;
        // Redirected to a file our stdout is block-buffered while the child writes straight
        // to the descriptor, so without this the report lands after the output it was
        // introducing.
        let _ = io::stdout().flush();
        let mut command = builder(program, arguments);
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        if let Some(directory) = cwd {
            command.current_dir(directory);
        }
        let status = command.status().map_err(|error| RunError {
            detail: errno_detail(program, &error),
        })?;
        Ok(exit_code(status))
    }
}

/// The half both detached spawns share: stdio to `/dev/null`, started and not waited for.
///
/// Detached means unobserved, so nothing here reports the child's exit status -- by
/// construction there is nobody left to report it to.
fn detached(command: &mut Command, program: &str) -> Result<(), RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_child| ())
        .map_err(|error| RunError {
            detail: errno_detail(program, &error),
        })
}

/// `setsid()`: leave the session this process was started in, for good.
///
/// The child half of [`Runner::spawn_self_detaching`], and between them they are
/// `start_new_session=True` with no `unsafe` anywhere. A process that calls this gets a
/// session of its own, becomes its leader, gets a process group of its own, and -- the part
/// that matters -- loses the controlling terminal. The `SIGHUP` a closing terminal sends to
/// its foreground process group can no longer reach it, which is the failure invariant 10
/// exists to prevent: a `garage display-test` run from a terminal that is then closed inside
/// the fifteen-second window must still be reverted.
///
/// `rustix::process::setsid` is a safe function -- the call has no memory-safety dimension at
/// all, which is why it needs no `unsafe` block here and why reaching for it is not a way
/// around the workspace's `forbid(unsafe_code)`.
///
/// The one failure it can report is `EPERM`, which means the caller is already a process
/// group leader; that is `Ok` from this function's point of view, because a leader of its own
/// group in its own right is already as detached as this can make it, and the caller has
/// nothing useful to do about it either way. There is no `Result` for the same reason.
///
/// # The residual window, named exactly
///
/// The child is in this process's session from `fork` until it reaches this call -- a few
/// milliseconds of `exec`, argument parsing and dispatch. A `SIGHUP` delivered inside that
/// window would still find it. Closing that would need `posix_spawn`'s `SETSID` flag or a
/// `pre_exec` hook, and neither is reachable without `unsafe` or a new dependency; the window
/// is bounded by one `exec` rather than by the fifteen seconds the child then sleeps for.
pub fn enter_new_session() {
    let _ = rustix::process::setsid();
}

/// The one `Command::new` in the workspace. See the module docs.
#[allow(
    clippy::disallowed_methods,
    reason = "this module is the process boundary the ban routes every other crate through"
)]
fn builder(program: &str, arguments: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.args(arguments);
    command
}

/// Poll until the child exits, or kill it once the deadline passes.
fn wait(
    child: &mut Child,
    program: &str,
    command: &[&str],
    timeout: Duration,
) -> Result<ExitStatus, RunError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                return Err(RunError {
                    detail: errno_detail(program, &error),
                })
            }
        }
        if Instant::now() >= deadline {
            // Kill, then reap. `subprocess` does the same in its own `finally`, and
            // leaving the child unreaped would leak a zombie for this process's lifetime.
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError {
                detail: timeout_detail(command, timeout),
            });
        }
        thread::sleep(POLL);
    }
}

/// `shutil.which()`, for the one caller that asks before it runs: [`crate::Luac`].
///
/// A name carrying a separator is checked where it stands, exactly as `shutil.which()` does;
/// anything else is looked for along `PATH`. Executability is `mode & 0o111` rather than
/// `access(X_OK)`, which differs only for a file this user could not execute despite its
/// bits -- a case that would fail at spawn time either way.
#[must_use]
pub fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let candidate = PathBuf::from(program);
        return executable(&candidate).then_some(candidate);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(program))
        .find(|candidate| executable(candidate))
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|data| data.is_file() && data.permissions().mode() & 0o111 != 0)
}

/// The argv split every shape here needs. An empty vector is a caller bug -- the Python
/// would raise `IndexError` out of `subprocess` -- and is reported rather than panicked on.
fn split<'a>(command: &'a [&'a str]) -> Result<(&'a str, &'a [&'a str]), RunError> {
    command.split_first().map_or_else(
        || {
            Err(RunError {
                detail: "a command with no program name cannot be run".to_owned(),
            })
        },
        |(program, arguments)| Ok((*program, arguments)),
    )
}

/// Read one pipe to the end on a thread of its own. See the module docs for why.
fn drain<R: Read + Send + 'static>(mut source: R) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = source.read_to_end(&mut buffer);
        // Lossy rather than strict: `text=True` decodes as UTF-8 and would raise on a
        // command that printed something else, and taking a whole settings write down
        // because `hyprctl` emitted one stray byte is not a trade this makes.
        String::from_utf8_lossy(&buffer).into_owned()
    })
}

/// A reader thread's result, with a thread that itself failed treated as no output.
fn collect(handle: Option<JoinHandle<String>>) -> String {
    handle
        .map(|thread| thread.join().unwrap_or_default())
        .unwrap_or_default()
}

/// `CompletedProcess.returncode`: the exit status, or the negated signal that killed it.
fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| -signal))
        .unwrap_or(-1)
}

/// `OSError.__str__`: `[Errno 2] No such file or directory: 'luac'`.
///
/// Rust spells the same failure `No such file or directory (os error 2)`, with the number
/// at the end rather than the front and no filename at all. The suffix is cut and the
/// pieces reassembled in the Python's order.
fn errno_detail(program: &str, error: &io::Error) -> String {
    let raw = error.to_string();
    let text = match raw.find(" (os error ") {
        Some(at) => raw.get(..at).unwrap_or(&raw),
        None => raw.as_str(),
    };
    match error.raw_os_error() {
        Some(code) => format!("[Errno {code}] {text}: {}", py_str_repr(program)),
        None => text.to_owned(),
    }
}

/// `TimeoutExpired.__str__`: `Command '['hyprctl', 'monitors', '-j']' timed out after 4
/// seconds`. The command is a Python list repr and the timeout is the number the caller
/// passed, spelled as Python spells it -- `4`, not `4.0`, for the whole-second default,
/// because `run()`'s parameter is annotated `float` but its default is the integer `4`.
fn timeout_detail(command: &[&str], timeout: Duration) -> String {
    let parts: Vec<String> = command.iter().copied().map(py_str_repr).collect();
    let spelled = if timeout.subsec_nanos() == 0 {
        timeout.as_secs().to_string()
    } else {
        py_float_repr(timeout.as_secs_f64())
    };
    format!(
        "Command '[{}]' timed out after {spelled} seconds",
        parts.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use garage_core::traits::{Runner as _, DEFAULT_RUN_TIMEOUT};

    use super::{errno_detail, timeout_detail, which, System};

    #[test]
    fn a_command_that_answers_comes_back_whole() {
        let output = System
            .run(
                &["/bin/sh", "-c", "printf out; printf err >&2"],
                DEFAULT_RUN_TIMEOUT,
            )
            .expect("/bin/sh runs");
        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, "out");
        assert_eq!(output.stderr, "err");
    }

    #[test]
    fn a_nonzero_exit_is_ok_with_a_status_rather_than_an_error() {
        let output = System
            .run(&["/bin/sh", "-c", "exit 3"], DEFAULT_RUN_TIMEOUT)
            .expect("/bin/sh runs");
        assert_eq!(output.status, 3);
    }

    #[test]
    fn a_missing_binary_reads_the_way_python_spells_it() {
        let error = System
            .run(&["/nonexistent/garage-test-binary"], DEFAULT_RUN_TIMEOUT)
            .expect_err("there is no such binary");
        assert_eq!(
            error.detail,
            "[Errno 2] No such file or directory: '/nonexistent/garage-test-binary'"
        );
    }

    #[test]
    fn a_child_that_never_answers_is_killed_and_reported() {
        let error = System
            .run(&["/bin/sh", "-c", "sleep 30"], Duration::from_millis(120))
            .expect_err("the sleep outlives the timeout");
        assert_eq!(
            error.detail,
            "Command '['/bin/sh', '-c', 'sleep 30']' timed out after 0.12 seconds"
        );
    }

    #[test]
    fn a_child_that_fills_a_pipe_does_not_deadlock_the_wait() {
        let output = System
            .run(
                &["/bin/sh", "-c", "yes hello | head -c 200000"],
                DEFAULT_RUN_TIMEOUT,
            )
            .expect("/bin/sh runs");
        assert_eq!(output.stdout.len(), 200_000);
    }

    #[test]
    fn a_detached_child_starts_and_is_not_waited_for() {
        System
            .spawn_detached(&["/bin/sh", "-c", "exit 0"])
            .expect("/bin/sh starts");
    }

    #[test]
    fn a_streamed_child_hands_back_its_status() {
        let status = System
            .run_streamed(&["/bin/sh", "-c", "exit 4"], None)
            .expect("/bin/sh runs");
        assert_eq!(status, 4);
    }

    #[test]
    fn a_killed_child_reports_the_negated_signal() {
        let output = System
            .run(&["/bin/sh", "-c", "kill -TERM $$"], DEFAULT_RUN_TIMEOUT)
            .expect("/bin/sh runs");
        assert_eq!(output.status, -15);
    }

    #[test]
    fn the_whole_second_default_is_spelled_without_a_decimal_point() {
        assert_eq!(
            timeout_detail(&["hyprctl"], DEFAULT_RUN_TIMEOUT),
            "Command '['hyprctl']' timed out after 4 seconds"
        );
    }

    #[test]
    fn an_error_with_no_errno_keeps_its_own_wording() {
        let error = std::io::Error::other("something went wrong");
        assert_eq!(errno_detail("x", &error), "something went wrong");
    }

    #[test]
    fn which_finds_a_path_it_is_handed_and_refuses_one_that_is_not_there() {
        assert!(which("/bin/sh").is_some());
        assert!(which("/nonexistent/garage-test-binary").is_none());
        assert!(which("garage-definitely-not-on-path").is_none());
    }

    #[test]
    fn an_empty_command_is_refused_rather_than_indexed() {
        assert!(System.run(&[], DEFAULT_RUN_TIMEOUT).is_err());
        assert!(System.spawn_detached(&[]).is_err());
        assert!(System.run_streamed(&[], None).is_err());
    }
}
