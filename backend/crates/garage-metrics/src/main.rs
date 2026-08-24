//! One collector behind both system-metrics surfaces in Garage.
//!
//! The bar and the popover used to be four unrelated scripts (activity-graph.py,
//! system-activity.py, gpu-activity.py, garage-vram-info) that each rediscovered the same
//! hardware and each disagreed about the answer. Every formula and every
//! device-detection rule below is lifted from those, because they were debugged against
//! this hardware over months; what is new here is that there is now exactly one of each.
//!
//! Three modes, and the reason each exists -- see [`modes`] for the long version.
//!
//! The rate metrics (CPU, network, disk) are counter deltas, so both snapshot modes have
//! to carry the previous counters in memory across reads. `--vram-info` is the
//! compatibility output for `AboutPalette` and needs no rate counters.
//!
//! Nothing outside the standard library and this workspace's own crates, plus nvidia-smi
//! and findmnt when they happen to be installed. This runs whenever the shell opens the
//! system popup and a dependency here is a dependency of the desktop coming up.
//!
//! The port keeps the Python's shape rather than improving on it, including the places
//! the Python is uneven: `stream` catches two of the five exception classes the old bar
//! tick caught. That is pinned by tests rather than tidied.

#![forbid(unsafe_code)]

mod dirs;
mod fault;
mod files;
mod json;
mod modes;
mod pyfmt;
mod snapshot;
mod sources;
mod state;

#[cfg(test)]
mod scratch;

use dirs::Dirs;
use std::io::Write as _;
use std::process::ExitCode;

/// What an argument this does not understand gets, on stderr, before exiting 2.
const USAGE: &str = concat!(
    "usage: garage-metrics --stream | --once | --vram-info\n",
    "\n",
    "  --stream    one JSON snapshot per line at 1 Hz, seeded from persisted history\n",
    "  --once      one JSON snapshot, then exit\n",
    "  --vram-info GPU name and VRAM bytes as tab-separated lines\n",
);

/// The exit status a mode that could not finish leaves behind -- Python's uncaught
/// exception.
const FAILED: u8 = 1;

/// The exit status for an argument this does not understand, which is the Python's own
/// `return 2` and what a shell expects from a usage error.
const MISUSE: u8 = 2;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().collect();
    let command = arguments.get(1).map_or("", String::as_str);
    if matches!(command, "-h" | "--help" | "help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if command == "--vram-info" {
        modes::vram_info();
        return ExitCode::SUCCESS;
    }
    let dirs = Dirs::from_env();
    let outcome = match command {
        "--stream" => modes::stream(&dirs),
        "--once" => modes::once(),
        _ => return misuse(),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        // The Python's traceback, minus the traceback: one line naming what failed, on
        // the stream the journal captures under the bar's units.
        Err(error) => {
            let mut stderr = std::io::stderr();
            let _ = writeln!(stderr, "garage-metrics: {error}");
            ExitCode::from(FAILED)
        }
    }
}

fn misuse() -> ExitCode {
    let mut stderr = std::io::stderr();
    let _ = write!(stderr, "{USAGE}");
    ExitCode::from(MISUSE)
}

#[cfg(test)]
mod tests {
    use super::USAGE;

    #[test]
    fn the_usage_lists_every_mode_and_no_retired_one() {
        assert!(USAGE.contains("--stream"));
        assert!(USAGE.contains("--once"));
        assert!(USAGE.contains("--vram-info"));
        assert!(!USAGE.contains("--bar-svg"));
    }

    #[test]
    fn the_usage_ends_in_a_newline_so_printing_it_adds_none() {
        assert!(USAGE.ends_with('\n'));
        assert!(!USAGE.ends_with("\n\n"));
    }
}
