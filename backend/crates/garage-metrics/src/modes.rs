//! The stream, one-shot snapshot, and VRAM compatibility modes.

use std::io::{self, Write as _};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::dirs::Dirs;
use crate::fault::Fault;
use crate::json::{dumps, object, Value};
use crate::snapshot::{now, Snapshotter};
use crate::sources::gpu;
use crate::state::History;

const STREAM_INTERVAL: Duration = Duration::from_secs(1);
const PRIME_DELAY: Duration = Duration::from_millis(250);

/// Anything that prevents a requested mode from finishing.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ModeError {
    #[error("{path}: {source}")]
    File { path: String, source: io::Error },
    #[error("{0}")]
    Sensor(#[from] Fault),
}

fn file_error(path: &Path, source: io::Error) -> ModeError {
    ModeError::File {
        path: path.display().to_string(),
        source,
    }
}

/// Emit a seed followed by one snapshot per second, persisting each successful frame.
///
/// # Errors
///
/// Returns a sensor error the stream protocol does not degrade, or a state/stdout write
/// error before the consumer has closed its pipe.
pub(crate) fn stream(dirs: &Dirs) -> Result<(), ModeError> {
    let mut history = History::load(&dirs.history);
    emit(&dumps(&history.seed(now()))).map_err(|source| ModeError::File {
        path: "<stdout>".to_owned(),
        source,
    })?;
    let mut snapshotter = Snapshotter::new();
    snapshotter.prime()?;
    let mut deadline = Instant::now();
    loop {
        deadline += STREAM_INTERVAL;
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let line = match snapshotter.snapshot() {
            Ok(snapshot) => {
                history.push_snapshot(&snapshot);
                history
                    .write(&dirs.history)
                    .map_err(|source| file_error(&dirs.history, source))?;
                dumps(&snapshot)
            }
            Err(error) if error.kind().caught_by_stream() => dumps(&Value::Object(object! {
                "ts" => Value::Float(now()),
                "error" => Value::str(error.to_string()),
            })),
            Err(error) => return Err(error.into()),
        };
        if emit(&line).is_err() {
            return Ok(());
        }
    }
}

fn emit(line: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{line}")?;
    stdout.flush()
}

/// Take and print one full snapshot.
///
/// # Errors
///
/// Returns any sensor error; this is the diagnostic mode, so it does not degrade.
pub(crate) fn once() -> Result<(), ModeError> {
    let mut snapshotter = Snapshotter::new();
    snapshotter.prime()?;
    std::thread::sleep(PRIME_DELAY);
    let snapshot = snapshotter.snapshot()?;
    let _ = emit(&dumps(&snapshot));
    Ok(())
}

/// Print GPU names and fitted VRAM as the legacy two-column protocol.
pub(crate) fn vram_info() {
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(vram_table(&gpu::discover()).as_bytes());
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
    use super::vram_table;
    use crate::sources::gpu::Gpu;

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
    fn vram_output_skips_cards_without_positive_capacity() {
        assert_eq!(
            vram_table(&[
                gpu("NVIDIA", Some(17_094_934_528)),
                gpu("Intel", None),
                gpu("Broken", Some(0)),
            ]),
            "NVIDIA\t17094934528\n"
        );
    }
}
