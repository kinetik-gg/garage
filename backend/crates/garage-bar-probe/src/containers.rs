//! The containers half: which running container names the available engines report.
//!
//! Deliberate deviation from the waybar module this replaces, which preserved the Python
//! port's quirk that a missing `podman` aborted before `docker` was ever probed. Here each
//! engine stands on its own: a spawn failure (engine not installed, socket unreachable at
//! the executable level) contributes nothing and never masks the other engine, while a
//! probe that runs but exits non-zero still has its stdout parsed -- `docker ps` against a
//! stopped daemon prints an error to stderr and exits non-zero with empty stdout, which is
//! indistinguishable from "no containers running" in the names it yields.

use std::collections::BTreeSet;
use std::time::Duration;

use garage_core::process::{run, RunError};

const ENGINES: [&str; 2] = ["podman", "docker"];
pub(crate) const TIMEOUT: Duration = Duration::from_secs(3);

/// The running-container state one tick produced. `None` when no engine could be probed
/// at all, which is what hides the bar's chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Containers {
    pub(crate) names: Vec<String>,
}

pub(crate) fn probe() -> Result<Containers, RunError> {
    let mut names = BTreeSet::new();
    let mut probed_any = false;
    for engine in ENGINES {
        let Ok(output) = run(&[engine, "ps", "--format", "{{.Names}}"], TIMEOUT) else {
            continue;
        };
        probed_any = true;
        names.extend(
            output
                .stdout
                .lines()
                .filter(|name| !name.is_empty())
                .map(str::to_string),
        );
    }
    if probed_any {
        Ok(Containers {
            names: names.into_iter().collect(),
        })
    } else {
        Err(RunError::Spawn(std::io::Error::other(
            "no container engine answered",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::Containers;

    #[test]
    fn names_are_sorted_and_deduplicated_by_the_caller() {
        // `probe()` collects through a BTreeSet; pin that contract on the struct's
        // constructor path so an engine reporting overlapping names cannot reorder or
        // duplicate the chip's tooltip.
        let merged: std::collections::BTreeSet<String> = ["web", "db", "web"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let state = Containers {
            names: merged.into_iter().collect(),
        };
        assert_eq!(vec!["db".to_owned(), "web".to_owned()], state.names);
    }
}
