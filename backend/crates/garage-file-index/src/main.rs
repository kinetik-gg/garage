//! Background filename index and low-latency query helper for Garage.
//!
//! The Rust port of `desktop/.local/bin/garage-file-index` (393 lines of Python). Four
//! commands, `argv[1]` defaulting to `run`:
//!
//!   * `run` -- the long-lived service `garage-file-index.service` starts: refresh, sleep,
//!     repeat, until `SIGTERM`/`SIGINT` or `[indexing]` turns itself off. See
//!     [`service::run_service`]. The one command that does not print a JSON envelope on
//!     success.
//!   * `refresh` -- one immediate rebuild of the whole index. See [`refresh::refresh_index`].
//!   * `search QUERY [LIMIT]` -- the launcher's read path, `LauncherSources.qml`'s
//!     `finishFileRequest`. See [`search::search_index`].
//!   * `status` -- the preferences pane's read path, `PreferencesController.qml`. See
//!     [`status::status`].
//!
//! Every command but `run` prints exactly one line of compact JSON --
//! `{"ok":...,"data":...,"error":...}`, via [`json::response`] -- whether it succeeds or
//! fails; `run`'s own failure still goes through the same envelope, since a `run_service`
//! error is caught by the same outer handler that wraps the other three in the Python.
#![forbid(unsafe_code)]

mod config;
mod db;
mod error;
mod fold;
mod json;
mod paths;
mod refresh;
mod scan;
mod search;
mod service;
mod status;

use error::{FileIndexError, USAGE};
use json::{response, Json};
use paths::IndexPaths;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let paths = IndexPaths::from_env();
    std::process::exit(dispatch(&paths, &args));
}

/// `main()`'s whole command table -- `argv[1]`, defaulting to `"run"`. `run` is dispatched
/// separately because it is the one command whose *success* skips
/// [`json::response`] entirely; its failure does not, matching the Python's `main()`, whose
/// `try` wraps the `run_service()` call along with the other three.
fn dispatch(paths: &IndexPaths, args: &[String]) -> i32 {
    let command = args.get(1).map_or("run", String::as_str);
    if command == "run" {
        return match service::run_service(paths) {
            Ok(code) => code,
            Err(error) => {
                response(None, &error.to_string());
                1
            }
        };
    }
    match run_command(paths, command, args) {
        Ok(data) => {
            response(Some(data), "");
            0
        }
        Err(error) => {
            response(None, &error.to_string());
            1
        }
    }
}

/// The three commands that answer through the JSON envelope, plus the catch-all that
/// reports an unrecognised one -- the Python's shadowed `IndexError`, named
/// [`FileIndexError::Usage`] here; see `error.rs` for why the rename is a deliberate, noted
/// deviation.
fn run_command(paths: &IndexPaths, command: &str, args: &[String]) -> Result<Json, FileIndexError> {
    match command {
        "refresh" => Ok(refresh::refresh_index(paths, None)?.to_json()),
        "search" => {
            let query = args.get(2).map_or("", String::as_str);
            let limit = match args.get(3) {
                Some(text) => text.parse::<i64>()?,
                None => 8,
            };
            let rows = search::search_index(paths, query, limit)?;
            Ok(Json::Object(vec![(
                "rows".to_string(),
                Json::Array(rows.iter().map(search::SearchHit::to_json).collect()),
            )]))
        }
        "status" => Ok(status::status(paths)?.to_json()),
        _ => Err(FileIndexError::Usage(USAGE)),
    }
}
