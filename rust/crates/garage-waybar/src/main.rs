//! `garage-waybar-module` -- Rust port of `desktop/.config/waybar/media-status.py`
//! and `desktop/.config/waybar/context-status.py`, as two subcommands:
//!
//! * `garage-waybar-module media [--activate] [PREFERRED]`
//! * `garage-waybar-module context {containers|microphone|smb}`
//!
//! The subcommand split itself has no Python analogue -- the two originals were
//! separate scripts, each its own process entry point -- so `main()`'s own dispatch
//! on `"media"`/`"context"` is new surface this port introduces, not a port of
//! anything. Everything reachable *after* that first argument is a behaviour-exact
//! port; see `src/media/mod.rs` and `src/context/mod.rs` for the two scripts' own
//! entry points and their documented deviations.
#![forbid(unsafe_code)]

mod context;
mod exec;
mod media;
mod waybar;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        Some("media") => {
            media::dispatch(rest);
            ExitCode::SUCCESS
        }
        Some("context") => context::dispatch(rest),
        _ => usage_error(),
    }
}

fn usage_error() -> ExitCode {
    eprintln!("usage: garage-waybar-module media [--activate] [PREFERRED]");
    eprintln!("       garage-waybar-module context {{containers|microphone|smb}}");
    ExitCode::from(2)
}
