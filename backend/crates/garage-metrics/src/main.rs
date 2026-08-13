//! One collector behind both system-metrics surfaces in Garage.
//!
//! The bar and the popover used to be four unrelated scripts (activity-graph.py,
//! system-activity.py, gpu-activity.py, garage-vram-info) that each rediscovered the same
//! hardware and each disagreed about the answer. Every formula, every device-detection
//! rule and every piece of SVG geometry below is lifted from those, because they were
//! debugged against this hardware over months; what is new here is that there is now
//! exactly one of each.
//!
//! Four modes, and the reason each exists -- see [`modes`] for the long version.
//!
//! The rate metrics (CPU, network, disk) are counter deltas, so every mode has to carry
//! the previous counters somewhere. `--bar-svg` carries them in the on-disk state file,
//! because each tick is a fresh process; `--stream` and `--once` carry them in memory.
//! `--vram-info` is the compatibility output for `AboutPalette` and needs no rate counters.
//!
//! Nothing outside the standard library and this workspace's own crates, plus nvidia-smi
//! and findmnt when they happen to be installed. This runs on the bar's interval and a
//! dependency here is a dependency of the desktop coming up.
//!
//! The port keeps the Python's shape rather than improving on it, including the places
//! the Python is uneven: `bar_svg` degrades a failed sensor and still exits 0, while an
//! unwritable state directory is a nonzero exit; and `stream` catches two of the five
//! exception classes `bar_svg` catches. Both are pinned by tests rather than tidied.

#![forbid(unsafe_code)]

mod data;
mod dirs;
mod exec;
mod fault;
mod files;
mod json;
mod modes;
mod pyfmt;
mod render;
mod sample;
mod snapshot;
mod sources;
mod state;

#[cfg(test)]
mod scratch;

use dirs::{Dirs, WIDGETS};
use std::io::Write as _;
use std::process::ExitCode;

/// What an argument this does not understand gets, on stderr, before exiting 2.
const USAGE: &str = concat!(
    "usage: garage-metrics --bar-svg <widget> | --stream | --once | --vram-info\n",
    "\n",
    "  --bar-svg <widget>  render one waybar strip; prints the SVG path then the tooltip\n",
    "  --stream            one JSON snapshot per line at 1 Hz, seeded from bar history\n",
    "  --once              one JSON snapshot, then exit\n",
    "  --vram-info         GPU name and VRAM bytes as tab-separated lines\n",
    "\n",
    "  widgets: cpu memory network temp disk gpu\n",
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
        "--bar-svg" => match arguments.get(2).map(String::as_str) {
            Some(widget) if WIDGETS.contains(&widget) => modes::bar_svg(&dirs, widget),
            _ => return misuse(),
        },
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
    use super::{USAGE, WIDGETS};

    #[test]
    fn the_usage_lists_every_widget_in_the_order_the_table_holds_them() {
        assert!(USAGE.contains("widgets: cpu memory network temp disk gpu"));
        assert!(USAGE.contains("--vram-info"));
        assert_eq!(WIDGETS.join(" "), "cpu memory network temp disk gpu");
    }

    #[test]
    fn the_usage_ends_in_a_newline_so_printing_it_adds_none() {
        assert!(USAGE.ends_with('\n'));
        assert!(!USAGE.ends_with("\n\n"));
    }
}
