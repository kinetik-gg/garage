//! Install ledger: durable authority for paths reconcile itself linked.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use garage_core::fs::atomic::atomic_write;
use garage_core::paths::Paths;
use serde::{Deserialize, Serialize};

use crate::error::ReconcileError;
use crate::path::safe_relative;

const VERSION: u32 = 1;

/// One path created or linked by reconcile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct LedgerEntry {
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) owner: Option<String>,
    pub(crate) timestamp: String,
}

/// Versioned ledger document at `~/.local/state/garage/manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Ledger {
    version: u32,
    pub(crate) paths: Vec<LedgerEntry>,
}

impl Default for Ledger {
    fn default() -> Self {
        Self {
            version: VERSION,
            paths: Vec::new(),
        }
    }
}

impl Ledger {
    pub(crate) fn load(paths: &Paths) -> Result<Self, ReconcileError> {
        let path = ledger_path(paths);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => return Err(ReconcileError::io("read ledger", &path, source)),
        };
        let ledger: Self = serde_json::from_str(&text)?;
        if ledger.version != VERSION {
            return Err(ReconcileError::LedgerVersion(ledger.version));
        }
        for entry in &ledger.paths {
            safe_relative(&entry.path)?;
        }
        Ok(ledger)
    }

    pub(crate) fn contains(&self, path: &str) -> bool {
        self.paths.iter().any(|entry| entry.path == path)
    }

    pub(crate) fn record(&mut self, entry: LedgerEntry) {
        self.paths.retain(|known| known.path != entry.path);
        self.paths.push(entry);
        self.paths.sort_by(|left, right| left.path.cmp(&right.path));
    }

    pub(crate) fn remove(&mut self, path: &str) {
        self.paths.retain(|entry| entry.path != path);
    }

    pub(crate) fn write(&self, paths: &Paths) -> Result<(), ReconcileError> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        atomic_write(&ledger_path(paths), &text)?;
        Ok(())
    }
}

pub(crate) fn ledger_path(paths: &Paths) -> PathBuf {
    paths.state_root.join("manifest.json")
}

pub(crate) fn log_path(paths: &Paths) -> PathBuf {
    paths.state_root.join("reconcile.log")
}
