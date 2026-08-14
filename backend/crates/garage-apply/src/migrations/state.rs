//! Versioned durable stamp for one-shot machine migrations.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use garage_core::fs::atomic::{atomic_write, AtomicWriteError};
use garage_core::paths::Paths;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::Outcome;

const VERSION: u32 = 1;

/// The whole document at [`Paths::migrations`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct State {
    version: u32,
    applied: Vec<Applied>,
}

/// One settled migration, retained as an audit record rather than only an id set.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Applied {
    id: String,
    timestamp: String,
    outcome: Outcome,
}

/// A stamp cannot be trusted or replaced.
#[derive(Debug, Error)]
pub(super) enum StateError {
    /// The existing stamp could not be read.
    #[error("{}: could not read the migration stamp: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The document is not the stamp schema this binary understands.
    #[error("unsupported migration stamp version {0}")]
    Version(u32),
    /// The document is not valid JSON or does not have the stamp's shape.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The complete replacement could not be published atomically.
    #[error(transparent)]
    Atomic(#[from] AtomicWriteError),
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: VERSION,
            applied: Vec::new(),
        }
    }
}

impl State {
    /// Read the stamp, treating absence as a machine with no settled migrations.
    ///
    /// An unknown version is refused rather than silently reset: resetting a one-way ledger
    /// would make every entry eligible to replay. This deliberately mirrors the version
    /// refusal in `garage-reconcile/src/ledger.rs`, whose ledger is durable authority too.
    pub(super) fn load(paths: &Paths) -> Result<Self, StateError> {
        let text = match fs::read_to_string(&paths.migrations) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(StateError::Read {
                    path: paths.migrations.clone(),
                    source,
                });
            }
        };
        let state: Self = serde_json::from_str(&text)?;
        if state.version != VERSION {
            return Err(StateError::Version(state.version));
        }
        Ok(state)
    }

    pub(super) fn contains(&self, id: &str) -> bool {
        self.applied.iter().any(|entry| entry.id == id)
    }

    /// Return a candidate next stamp; the caller publishes it before replacing live state.
    pub(super) fn recorded(&self, id: &str, outcome: Outcome) -> Self {
        let seconds = garage_core::time::now_seconds();
        let mut next = self.clone();
        next.applied.push(Applied {
            id: id.to_owned(),
            timestamp: garage_core::time::local_iso8601(seconds),
            outcome,
        });
        next
    }

    /// Atomically publish pretty JSON with the repository's text-file newline convention.
    pub(super) fn write(&self, paths: &Paths) -> Result<(), StateError> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        atomic_write(&paths.migrations, &text)?;
        Ok(())
    }
}
