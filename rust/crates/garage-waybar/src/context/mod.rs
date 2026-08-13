//! `garage-waybar-module context {containers|microphone|smb}` -- the whole of
//! `context-status.py`.
//!
//! The Python's dispatch is `MODES[sys.argv[1]]()` inside `try: ... except
//! (IndexError, OSError, subprocess.SubprocessError): emit()`. That tuple is
//! deliberate and asymmetric, and this port keeps the asymmetry:
//!
//! * No argument at all (`sys.argv[1]` raising `IndexError`) is caught -- an empty
//!   payload, exit 0. [`dispatch`] mirrors this with its own `rest.first()` check.
//! * A spawn failure or a timeout from any probe (`OSError`/`SubprocessError`) is
//!   caught -- an empty payload, exit 0.
//! * An argument that is present but not one of the three keys
//!   (`MODES["nonsense"]`) is a **`KeyError`**, which is in neither exception class
//!   the `except` names. It is NOT caught: the Python crashes with a traceback on
//!   stderr and a non-zero exit, printing no JSON at all. [`dispatch`] reproduces
//!   that distinction by exiting non-zero and never calling [`Payload::emit`] for an
//!   unrecognised mode, rather than folding it into the same "empty payload" path an
//!   ordinary probe failure takes.

mod containers;
mod microphone;
mod run;
mod smb;
mod theme;

use std::process::ExitCode;

use garage_core::paths::Paths;

use crate::exec::RunError;
use crate::waybar::Payload;

pub(crate) fn dispatch(rest: &[String]) -> ExitCode {
    let Some(mode) = rest.first() else {
        Payload::idle().emit();
        return ExitCode::SUCCESS;
    };
    match mode.as_str() {
        "containers" => emit_or_idle(containers::payload()),
        "microphone" => emit_or_idle(microphone::payload(&Paths::from_env())),
        "smb" => emit_or_idle(smb::payload(&Paths::from_env())),
        other => unknown_mode(other),
    }
}

/// A probe that ran returns `Ok`; a spawn failure or timeout returns `Err` and is
/// folded into an idle payload here -- the `except (OSError,
/// subprocess.SubprocessError)` half of the Python's catch.
fn emit_or_idle(result: Result<Payload, RunError>) -> ExitCode {
    result.unwrap_or_else(|_| Payload::idle()).emit();
    ExitCode::SUCCESS
}

/// The uncaught-`KeyError` case: see the module docs. No payload is printed.
fn unknown_mode(mode: &str) -> ExitCode {
    eprintln!("garage-waybar-module: context: unknown mode {mode:?}");
    ExitCode::FAILURE
}
