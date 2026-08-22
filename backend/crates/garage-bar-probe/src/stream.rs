//! The emission loop: one merged JSON state per tick, containers-paced.
//!
//! Cadence contract: a line lands on stdout every [`CONTAINER_INTERVAL`] seconds. The SMB
//! half is re-probed only every [`SMB_EVERY_TICKS`] ticks -- `gio mount -l` costs a D-Bus
//! round trip, and share mounts change on the order of minutes, not seconds -- so between
//! its own probes the previous SMB state rides along unchanged rather than going stale to
//! `null`.
//!
//! A failed containers probe reports `"containers": null` for that tick without touching
//! the SMB half, and vice versa: each section stands on its own, which is what lets the
//! bar hide one chip while the other keeps drawing.
//!
//! A write to a closed stdout ends the process quietly -- the bar that was reading is
//! gone, and nobody is left to report to.

use std::io::Write as _;
use std::time::Duration;

use garage_core::paths::Paths;
use serde_json::json;

use crate::containers::{self, Containers};
use crate::microphone::{self, Microphone};
use crate::smb::{self, Smb};

/// Seconds between emitted lines; also the containers probe cadence.
const CONTAINER_INTERVAL: Duration = Duration::from_secs(5);
/// SMB re-probes are this many ticks apart (so 15 s at the shipped interval).
const SMB_EVERY_TICKS: u32 = 3;

/// One tick's worth of context state.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    containers: Option<Containers>,
    smb: Option<Smb>,
    mic: Option<Microphone>,
}

impl State {
    fn empty() -> Self {
        Self {
            containers: None,
            smb: None,
            mic: None,
        }
    }
}

/// The wire shape the bar parses. Keys land wherever `serde_json`'s ordered map puts
/// them; the bar reads by name.
fn to_value(state: &State) -> serde_json::Value {
    let containers = state.containers.as_ref().map(|found| {
        json!({
            "running": found.names.len(),
            "names": found.names,
        })
    });
    let smb = state
        .smb
        .as_ref()
        .filter(|found| found.available)
        .map(|found| {
            json!({
                "expected": found.expected,
                "connected": found.connected,
                "missing_labels": found.missing_labels,
            })
        });
    let mic = state.mic.as_ref().map(|found| {
        json!({
            "recording": found.recording,
            "descriptions": found.descriptions,
        })
    });
    json!({ "containers": containers, "smb": smb, "mic": mic })
}

fn emit_line(state: &State) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{value}", value = to_value(state))?;
    stdout.flush()
}

fn probe_all(paths: &Paths) -> State {
    let mut state = State::empty();
    if let Ok(found) = containers::probe() {
        state.containers = Some(found);
    }
    // An unavailable helper is carried as `Some(none)` rather than dropped: it is a
    // real answer ("this machine manages no shares") and keeps the tick's shape stable.
    if let Ok(found) = smb::probe(paths) {
        state.smb = Some(found);
    }
    if let Ok(found) = microphone::probe() {
        state.mic = Some(found);
    }
    state
}

/// `garage-bar-probe once`: one line, then exit. The smoke-test entry point.
pub(crate) fn once() -> std::process::ExitCode {
    let paths = Paths::from_env();
    let state = probe_all(&paths);
    let _ = emit_line(&state);
    std::process::ExitCode::SUCCESS
}

/// `garage-bar-probe stream`: emit forever at the cadence contract above.
pub(crate) fn run() -> std::process::ExitCode {
    let paths = Paths::from_env();
    let mut ticks_until_smb = 0_u32;
    let mut last_smb: Option<Smb> = None;
    loop {
        let mut state = State {
            containers: None,
            smb: last_smb.clone(),
            mic: None,
        };
        if let Ok(found) = containers::probe() {
            state.containers = Some(found);
        }
        // The mic rides the full cadence: recording can start at any moment, and one
        // `pactl` call per tick is cheaper than the old module's two-second poll.
        if let Ok(found) = microphone::probe() {
            state.mic = Some(found);
        }
        if ticks_until_smb == 0 {
            if let Ok(found) = smb::probe(&paths) {
                // An unavailable helper is a real answer ("no managed shares"), so it
                // is carried like any other and keeps the emitted shape stable.
                last_smb = Some(found);
            }
            ticks_until_smb = SMB_EVERY_TICKS;
        } else {
            ticks_until_smb -= 1;
        }
        if emit_line(&state).is_err() {
            return std::process::ExitCode::SUCCESS;
        }
        std::thread::sleep(CONTAINER_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::{to_value, Containers, Microphone, Smb, State};
    use serde_json::json;

    #[test]
    fn a_full_state_serializes_every_section() {
        let state = State {
            containers: Some(Containers {
                names: vec!["db".to_owned(), "web".to_owned()],
            }),
            smb: Some(Smb {
                available: true,
                expected: 2,
                connected: 2,
                missing_labels: Vec::new(),
            }),
            mic: Some(Microphone {
                recording: false,
                descriptions: Vec::new(),
            }),
        };
        assert_eq!(
            to_value(&state),
            json!({
                "containers": { "running": 2, "names": ["db", "web"] },
                "smb": { "expected": 2, "connected": 2, "missing_labels": [] },
                "mic": { "recording": false, "descriptions": [] },
            })
        );
    }

    #[test]
    fn an_absent_probe_is_null_not_an_empty_object() {
        let state = State {
            containers: None,
            smb: Some(Smb {
                available: false,
                ..Smb::none()
            }),
            mic: None,
        };
        assert_eq!(
            to_value(&state),
            json!({ "containers": null, "smb": null, "mic": null })
        );
    }
}
