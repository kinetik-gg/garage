//! `containers()`: how many container engines have something running.
//!
//! Ported quirk, reported: the Python's `run()` helper for this script does not
//! check the exit code (`return subprocess.run(command, ...).stdout`, no `if
//! result.returncode == 0` guard the way `media-status.py`'s own `run()` has), and the
//! `for engine in ("podman", "docker")` loop has no per-engine try/except. So if
//! `podman` is not installed, `run(["podman", ...])` raises `OSError`
//! (`FileNotFoundError`) that propagates straight out of `containers()`, is never
//! caught locally, and is only caught by the module-level try/except -- meaning a
//! missing `podman` produces an EMPTY payload even when `docker` is present and has
//! containers running. `docker` is never reached. This port reproduces that ordering
//! and that failure exactly via `?`.

use std::time::Duration;

use crate::context::run::run_ignoring_status;
use crate::exec::RunError;
use crate::waybar::Payload;

const ENGINES: [&str; 2] = ["podman", "docker"];
const TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) fn payload() -> Result<Payload, RunError> {
    let mut names = std::collections::BTreeSet::new();
    for engine in ENGINES {
        let output = run_ignoring_status(&[engine, "ps", "--format", "{{.Names}}"], TIMEOUT)?;
        names.extend(
            output
                .lines()
                .filter(|name| !name.is_empty())
                .map(str::to_string),
        );
    }
    Ok(if names.is_empty() {
        Payload::idle()
    } else {
        running_payload(&names)
    })
}

fn running_payload(names: &std::collections::BTreeSet<String>) -> Payload {
    let list = names.iter().cloned().collect::<Vec<_>>().join("\n");
    Payload {
        text: format!("CTR {}", names.len()),
        tooltip: format!("Running containers\n{list}"),
        class: "active".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::running_payload;
    use std::collections::BTreeSet;

    #[test]
    fn the_running_payload_counts_and_lists_names_sorted() {
        let names: BTreeSet<String> = ["web", "db"].into_iter().map(str::to_string).collect();
        let payload = running_payload(&names);
        assert_eq!("CTR 2", payload.text);
        assert_eq!("Running containers\ndb\nweb", payload.tooltip);
        assert_eq!("active", payload.class);
    }
}
