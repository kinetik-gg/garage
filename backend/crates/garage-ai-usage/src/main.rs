//! AI subscription usage widget data source, via Tokscale. Port of
//! `desktop/.local/bin/garage-ai-usage`.
//!
//! Modes:
//!   `--bar`    Waybar custom-module payload (return-type json). The text is one Phosphor
//!              glyph; every figure is in the tooltip.
//!   `--json`   Popover payload: subscriptions usage + today's token/cost totals.
//!   `--probe`  Locate the tokscale CLI; exit 0 + path, or exit 1 + reason.
//!
//! Tokscale is optional. When it cannot be found, `--bar` prints an empty text (so
//! `hide-empty-text` hides the module) and exits 0, `--probe` exits nonzero with a reason,
//! and `--json` reports `{"available": false}`. Absence is a normal state, never an error
//! exit -- see [`output::build_bar_output`] and [`output::build_json_output`].
#![forbid(unsafe_code)]

mod cache;
mod exec;
mod output;
mod pyjson;
mod shape;
mod timeutil;
mod tokscale;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use cache::CachePaths;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let program = args.first().map_or("garage-ai-usage", String::as_str);
    let mode = args.get(1).map_or("--bar", String::as_str);

    let home = home_dir();
    let candidates = tokscale::tokscale_candidates(home.as_deref());
    let path_env = std::env::var("PATH").ok();
    let tokscale_path = tokscale::find_tokscale(&candidates, path_env.as_deref());
    let paths = CachePaths::new(cache_dir(home.as_deref()));

    let code = match mode {
        "--probe" => output::run_probe(tokscale_path.as_deref()),
        "--json" => {
            let value =
                output::build_json_output(tokscale_path.as_deref(), &paths, fetched_at_now());
            println!("{}", pyjson::to_python_json(&value));
            0
        }
        "--bar" => {
            let value =
                output::build_bar_output(tokscale_path.as_deref(), &paths, epoch_seconds_now());
            println!("{}", pyjson::to_python_json(&value));
            0
        }
        _ => {
            eprintln!("usage: {program} [--bar|--json|--probe]");
            2
        }
    };

    // `code` is always one of 0, 1 or 2 -- every branch above sets it from a literal or
    // from `output::run_probe`, which only ever returns 0 or 1.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    ExitCode::from(code as u8)
}

/// `Path.home()`: `$HOME`, and nothing else. A machine with no `$HOME` set falls back to a
/// `tokscale`-on-`PATH`-only search and a cache directory relative to the working
/// directory, rather than querying the passwd database the way Python's `Path.home()`
/// eventually would -- see the crate's port report for why that gap is accepted rather
/// than closed with an FFI call this lint wall forbids (`unsafe_code = "forbid"`).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn cache_dir(home: Option<&std::path::Path>) -> PathBuf {
    match home {
        Some(home) => home.join(".cache/garage-ai-usage"),
        None => PathBuf::from(".cache/garage-ai-usage"),
    }
}

/// `datetime.now(timezone.utc).isoformat()`, for `build_json_output()`'s `fetched_at`.
fn fetched_at_now() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // Seconds since 1970 fit comfortably in `i64` until the year 292 billion.
    #[allow(clippy::cast_possible_wrap)]
    let epoch_seconds = since_epoch.as_secs() as i64;
    timeutil::format_fetched_at_utc(epoch_seconds, since_epoch.subsec_micros())
}

/// The current wall clock as seconds since the epoch, for `reset_days()`'s `now`.
fn epoch_seconds_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64())
}
