//! Modes
//!
//! The four things this binary can be asked to do, and the reason each exists.
//!
//!   `--bar-svg <widget>`  One shot per waybar tick. Waybar's image module runs a
//!                         command on an interval and reads two lines back: a path to an
//!                         image, then the tooltip. So this mode has to do the whole job
//!                         -- sample, fold into history, render, and exit -- inside one
//!                         tick, which is why nothing here reaches a source the requested
//!                         widget does not need. nvidia-smi costs ~28ms and the CPU
//!                         widget must not pay it.
//!
//!   `--stream`            The Quickshell popover. One JSON object per line at 1 Hz,
//!                         prefixed by a seed line built from the bar's own history files
//!                         so the popover's graphs open already populated instead of
//!                         drawing themselves in over four minutes.
//!
//!   `--once`              One snapshot, then exit. Smoke tests and scripts.
//!
//!   `--vram-info`         The two-column compatibility protocol `AboutPalette` already reads.
//!                         It uses this collector's vendor discovery instead of probing the
//!                         same hardware a second way in shell.
//!
//! The rate metrics (CPU, network, disk) are counter deltas, so every mode has to carry
//! the previous counters somewhere. `--bar-svg` carries them in the on-disk state file,
//! because each tick is a fresh process; `--stream` and `--once` carry them in memory.
//! That is the only structural difference between the two halves.

use crate::dirs::Dirs;
use crate::fault::Fault;
use crate::json::{dumps, object, Value};
use crate::render::render_svg;
use crate::snapshot::{now, seed_object, Snapshotter};
use crate::sources::gpu;
use crate::state::{load_state, mark_unavailable, tooltip_for, update_state};
use garage_core::fs::atomic::atomic_write;
use std::fs::OpenOptions;
use std::io::{self, Write as _};
use std::path::Path;
use std::time::{Duration, Instant};

/// How often `--stream` emits an object.
const STREAM_INTERVAL: Duration = Duration::from_secs(1);

/// A counter delta needs two reads. `--once` has no earlier process to inherit them
/// from, so it takes both itself and pays this in latency.
const PRIME_DELAY: Duration = Duration::from_millis(250);

/// Anything that stops a mode finishing. Every one of these is a traceback in the
/// Python: they are outside the `try` that degrades a widget, and they end the process
/// with a nonzero status.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ModeError {
    /// A directory could not be created, a lock could not be taken, or a file could not
    /// be replaced.
    #[error("{path}: {source}")]
    File {
        /// What was being worked on.
        path: String,
        /// The underlying failure.
        source: io::Error,
    },
    /// A sensor failed in a mode that does not degrade -- `--once`, or one of the three
    /// exception classes `--stream` does not catch.
    #[error("{0}")]
    Sensor(#[from] Fault),
}

fn file_error(path: &Path, source: io::Error) -> ModeError {
    ModeError::File {
        path: path.display().to_string(),
        source,
    }
}

/// Render one waybar strip: sample, fold into history, render, print the two lines.
///
/// Never returns an error for a sensor that failed -- waybar treats a nonzero exit as a
/// broken module and stops calling it at all, so a missing GPU degrades the widget to
/// "n/a" and the tick still succeeds. It *does* return an error for a state directory it
/// cannot write, which is what the Python does too: `STATE_DIR.mkdir` and the lock's
/// `open` are both outside the `try`, and a permission failure there is a traceback and
/// an exit status of 1. That asymmetry is deliberate -- a widget with no sensor is a
/// normal machine, and a state directory that refuses writes is a broken install.
///
/// # Errors
///
/// Returns [`ModeError`] when the state or cache directory cannot be created, the lock
/// cannot be opened or taken, or either file cannot be replaced.
pub(crate) fn bar_svg(dirs: &Dirs, widget: &str) -> Result<(), ModeError> {
    let (path, tooltip) = tick(dirs, widget)?;
    // Waybar's image module reads exactly two lines: the path, then the tooltip. Nothing
    // else may reach stdout in this mode -- a third line is an image module that stops
    // rendering.
    println!("{path}");
    println!("{tooltip}");
    Ok(())
}

/// The tick itself: everything `bar_svg` does except the printing, so a test can hold
/// the two lines rather than capture a process's stdout.
fn tick(dirs: &Dirs, widget: &str) -> Result<(String, String), ModeError> {
    std::fs::create_dir_all(&dirs.state).map_err(|error| file_error(&dirs.state, error))?;
    std::fs::create_dir_all(&dirs.cache).map_err(|error| file_error(&dirs.cache, error))?;
    let state_path = dirs.state_file(widget);
    let svg_path = dirs.svg_file(widget);

    let state = {
        // The lock is held for exactly as long as this block: two waybar ticks can
        // overlap, and a lost update is a visible notch in the graph.
        let _lock = Lock::take(&dirs.lock_file(widget))?;
        let mut state = load_state(&state_path);
        if let Err(error) = update_state(widget, &mut state, now()) {
            mark_unavailable(widget, &mut state, &error);
        }
        let rendered = render_svg(widget, &state, &dirs.foreground);
        write_atomically(&state_path, &dumps(&Value::Object(state.clone())))?;
        write_atomically(&svg_path, &rendered)?;
        state
    };

    Ok((svg_path.display().to_string(), tooltip_for(widget, &state)))
}

/// An exclusive advisory lock, released when the handle is dropped.
///
/// Opened `a+` as the Python does: create if absent, and never truncate, because the
/// file's contents are irrelevant and truncating one another process is holding open
/// would be a pointless write.
#[derive(Debug)]
struct Lock(std::fs::File);

impl Lock {
    fn take(path: &Path) -> Result<Self, ModeError> {
        let handle = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)
            .map_err(|error| file_error(path, error))?;
        rustix::fs::flock(&handle, rustix::fs::FlockOperation::LockExclusive)
            .map_err(|error| file_error(path, io::Error::from(error)))?;
        Ok(Self(handle))
    }
}

/// Releasing the lock is what leaving the Python's `with` block does, by closing the
/// file. Dropping the handle would do it here too -- an `flock` is released when the
/// last descriptor referring to it closes -- but unlocking first says so out loud, and
/// keeps the release at a point in the code rather than at a point in the borrow
/// checker's reasoning.
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}

/// Replace a file in one step, so no reader ever sees a half-written one.
///
/// Waybar reads the SVG the instant the path is printed, and the popover reads the
/// history files while the bar is writing them. A partial file would be read as a broken
/// image or as corrupt JSON, so the write lands under a temporary name and arrives whole.
fn write_atomically(path: &Path, text: &str) -> Result<(), ModeError> {
    atomic_write(path, text).map_err(|error| ModeError::File {
        path: path.display().to_string(),
        source: io::Error::other(error.to_string()),
    })
}

/// One JSON snapshot per line at 1 Hz, seeded from bar history.
///
/// # Errors
///
/// Returns [`ModeError`] when priming fails, or when a snapshot fails with one of the
/// three exception classes the Python's `except (OSError, ValueError)` does not catch.
pub(crate) fn stream(dirs: &Dirs) -> Result<(), ModeError> {
    // Outside any try in the Python, so a popover that closed before the seed landed is
    // a traceback and a nonzero exit rather than a clean one. Kept as it is: nothing
    // reads that status, and inventing a friendlier answer here would be inventing.
    emit(&dumps(&seed_object(dirs))).map_err(|source| ModeError::File {
        path: "<stdout>".to_string(),
        source,
    })?;
    let mut snapshotter = Snapshotter::new();
    snapshotter.prime()?;
    // Paced against a deadline rather than sleeping a flat second, because a snapshot
    // costs ~30ms (nvidia-smi) and sleeping after it makes every object 1.03s apart. The
    // popover plots these on a fixed 1 Hz axis, so that drift is a graph that runs slow
    // by three percent forever.
    let mut deadline = Instant::now();
    loop {
        deadline += STREAM_INTERVAL;
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let line = match snapshotter.snapshot() {
            Ok(snapshot) => dumps(&snapshot),
            Err(error) if error.kind().caught_by_stream() => dumps(&Value::Object(object! {
                "ts" => Value::Float(now()),
                "error" => Value::str(error.to_string()),
            })),
            Err(error) => return Err(error.into()),
        };
        if emit(&line).is_err() {
            // The popover closed. Nothing to report and nowhere to report it.
            return Ok(());
        }
    }
}

/// One line to stdout, flushed. An error here is a closed pipe, which both callers treat
/// as a clean exit rather than a failure.
fn emit(line: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{line}")?;
    stdout.flush()
}

/// One JSON snapshot, then exit.
///
/// # Errors
///
/// Returns [`ModeError`] for any sensor failure at all. Nothing catches anything here in
/// the Python, so `--once` on a machine with a broken `/proc` is a traceback -- which is
/// the right answer for a smoke test.
pub(crate) fn once() -> Result<(), ModeError> {
    let mut snapshotter = Snapshotter::new();
    snapshotter.prime()?;
    std::thread::sleep(PRIME_DELAY);
    let snapshot = snapshotter.snapshot()?;
    let _ = emit(&dumps(&snapshot));
    Ok(())
}

/// GPU names and fitted VRAM as the two-column protocol `AboutPalette` already consumes.
pub(crate) fn vram_info() {
    let table = vram_table(&gpu::discover());
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(table.as_bytes());
}

fn vram_table(gpus: &[gpu::Gpu]) -> String {
    let mut table = String::new();
    for card in gpus {
        let Some(total) = card.vram_total.filter(|total| *total > 0) else {
            continue;
        };
        table.push_str(&card.name);
        table.push('\t');
        table.push_str(&total.to_string());
        table.push('\n');
    }
    table
}

#[cfg(test)]
mod tests {
    use super::{bar_svg, vram_table, Lock};
    use crate::dirs::Dirs;
    use crate::json::{dumps, object, Value};
    use crate::scratch::Scratch;
    use crate::sources::gpu::Gpu;
    use std::fs;

    /// A state file whose `last_sample` is far in the future, so `update_state`'s
    /// freshness guard always holds and the run renders from stored history without
    /// touching a single sensor. This is the trick the whole parity harness rests on.
    fn preseed(dirs: &Dirs, widget: &str, history: Vec<Value>) {
        fs::create_dir_all(&dirs.state).expect("mkdir");
        let state = object! {
            "history" => Value::List(history),
            "display" => Value::str("42%"),
            "extra" => Value::str(""),
            "tooltip_parts" => Value::strings(["CPU 42.0%"]),
            "active" => Value::Bool(true),
            "last_sample" => Value::Float(1e12),
            "period" => Value::Float(2.0),
        };
        fs::write(dirs.state_file(widget), dumps(&Value::Object(state))).expect("write");
    }

    fn gpu(name: &str, total: Option<i64>) -> Gpu {
        Gpu {
            name: name.to_owned(),
            vendor: "amd",
            load: None,
            load_kind: None,
            vram_used: None,
            vram_total: total,
            temp_c: None,
            discrete: total.is_some(),
        }
    }

    #[test]
    fn vram_compatibility_output_is_tab_separated_and_skips_missing_capacity() {
        assert_eq!(
            vram_table(&[
                gpu("NVIDIA GeForce RTX 5080", Some(17_094_934_528)),
                gpu("Intel Graphics", None),
                gpu("Broken reading", Some(0)),
                gpu("AMD Radeon Graphics", Some(25_769_803_776)),
            ]),
            "NVIDIA GeForce RTX 5080\t17094934528\nAMD Radeon Graphics\t25769803776\n"
        );
    }

    #[test]
    fn a_preseeded_run_writes_the_svg_and_leaves_the_state_untouched() {
        let scratch = Scratch::new("bar-svg");
        let dirs = Dirs::scratch(scratch.path());
        preseed(&dirs, "cpu", vec![Value::Float(50.0); 120]);
        let before = fs::read_to_string(dirs.state_file("cpu")).expect("read");

        bar_svg(&dirs, "cpu").expect("a preseeded run cannot fail");

        assert_eq!(
            fs::read_to_string(dirs.state_file("cpu")).expect("read"),
            before
        );
        let svg = fs::read_to_string(dirs.svg_file("cpu")).expect("svg written");
        assert!(svg.starts_with("<svg xmlns="));
        assert!(svg.ends_with("</svg>\n"));
        assert!(svg.contains(">42%<"));
    }

    #[test]
    fn the_two_lines_are_the_svg_path_then_the_tooltip_and_neither_wraps() {
        let scratch = Scratch::new("bar-protocol");
        let dirs = Dirs::scratch(scratch.path());
        preseed(&dirs, "cpu", vec![Value::Float(50.0); 120]);

        let (path, tooltip) = super::tick(&dirs, "cpu").expect("a preseeded run cannot fail");

        assert_eq!(path, dirs.svg_file("cpu").display().to_string());
        // The whole multi-part tooltip on one physical line, separators and all: waybar
        // reads exactly one line and a newline here would truncate it.
        assert_eq!(tooltip, "CPU 42.0% | last 4.0 min");
        assert!(!path.contains('\n'));
        assert!(!tooltip.contains('\n'));
    }

    #[test]
    fn every_widget_prints_a_path_and_a_tooltip_whatever_this_machine_has() {
        let scratch = Scratch::new("bar-all");
        let dirs = Dirs::scratch(scratch.path());
        for widget in crate::dirs::WIDGETS {
            let (path, tooltip) = super::tick(&dirs, widget)
                .unwrap_or_else(|error| panic!("{widget} must not fail: {error}"));
            assert!(path.ends_with(&format!("{widget}.svg")), "{widget}: {path}");
            assert!(!tooltip.is_empty(), "{widget} printed an empty tooltip");
            assert!(!tooltip.contains('\n'), "{widget}: {tooltip}");
        }
    }

    #[test]
    fn the_lock_file_is_created_beside_the_state_and_never_truncates_it() {
        let scratch = Scratch::new("bar-lock");
        let dirs = Dirs::scratch(scratch.path());
        fs::create_dir_all(&dirs.state).expect("mkdir");
        fs::write(dirs.lock_file("cpu"), "existing").expect("write");

        let lock = Lock::take(&dirs.lock_file("cpu")).expect("lock is takeable");
        drop(lock);

        assert_eq!(
            fs::read_to_string(dirs.lock_file("cpu")).expect("read"),
            "existing"
        );
    }

    #[test]
    fn a_state_directory_that_cannot_be_created_is_an_error_not_a_silent_success() {
        let scratch = Scratch::new("bar-readonly");
        let blocked = scratch.join("blocked");
        fs::write(&blocked, "not a directory").expect("write");
        let dirs = Dirs::scratch(&blocked);

        assert!(bar_svg(&dirs, "cpu").is_err());
    }

    #[test]
    fn an_unavailable_widget_still_writes_both_files_and_reports_success() {
        let scratch = Scratch::new("bar-unavailable");
        let dirs = Dirs::scratch(scratch.path());
        fs::create_dir_all(&dirs.state).expect("mkdir");
        // No preseed and no override: whatever this machine's disk does, the run has to
        // finish and leave both files behind.
        bar_svg(&dirs, "disk").expect("a bar tick never fails on a sensor");
        assert!(dirs.state_file("disk").exists());
        assert!(dirs.svg_file("disk").exists());
    }
}
