//! Command-line entry point for Garage.
//!
//! `raise SystemExit(main(sys.argv))`, and nothing else lives here: the argument vector is
//! collected, [`commands::run`] decides what to do with it and what status to leave with,
//! and this turns that status into an [`ExitCode`]. Not `std::process::exit`, which the
//! workspace's clippy configuration denies for the reason it usually is denied -- it skips
//! every destructor between here and the exit, and the preferences lock is one of them.
#![forbid(unsafe_code)]

mod commands;
mod error;
mod pyjson;
mod response;
mod set;

use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(commands::run(&std::env::args().collect::<Vec<String>>()))
}
