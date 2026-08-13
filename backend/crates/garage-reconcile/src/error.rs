//! Failures at the manifest and read-only diff boundary.

use std::path::PathBuf;

use thiserror::Error;

/// Why a reconcile plan could not be assembled or applied.
#[derive(Debug, Error)]
pub enum ReconcileError {
    /// Checkout discovery failed before any filesystem action was planned.
    #[error(transparent)]
    Checkout(#[from] garage_core::checkout::CheckoutError),
    /// One of the three source manifests could not be read or parsed.
    #[error(transparent)]
    Manifest(#[from] garage_core::manifest::ManifestError),
    /// A managed path escaped `$HOME` and is never safe to join or prune.
    #[error("managed path is not a safe HOME-relative path: {}", path.display())]
    UnsafePath {
        /// The rejected manifest value.
        path: PathBuf,
    },
    /// The only settled stow tree has a different manifest spelling.
    #[error("unsupported stow tree {0:?}; expected \"desktop/\"")]
    StowTree(String),
    /// A ledger version from a newer or invalid implementation is never guessed at.
    #[error("unsupported reconcile ledger version {0}")]
    LedgerVersion(u32),
    /// JSON in the ledger could not be decoded or a report could not be encoded.
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// An atomic ledger replacement failed.
    #[error(transparent)]
    Atomic(#[from] garage_core::fs::atomic::AtomicWriteError),
    /// An ordinary filesystem operation failed at a named path.
    #[error("{operation} {}: {source}", path.display())]
    Io {
        /// Human-readable operation in progress.
        operation: &'static str,
        /// Target of that operation.
        path: PathBuf,
        /// Operating-system refusal.
        source: std::io::Error,
    },
    /// A symlinked parent could redirect a planned move or link into the checkout itself.
    #[error("refusing to modify {} through symlinked parent {}", path.display(), parent.display())]
    SymlinkAncestor {
        /// Planned target.
        path: PathBuf,
        /// Parent that redirects traversal.
        parent: PathBuf,
    },
    /// A plan assembled without the operand its action requires.
    #[error("invalid reconcile plan for {path}: {detail}")]
    InvalidPlan {
        /// HOME-relative plan target.
        path: String,
        /// Missing or contradictory field.
        detail: &'static str,
    },
}

impl ReconcileError {
    pub(crate) fn io(
        operation: &'static str,
        path: &std::path::Path,
        source: std::io::Error,
    ) -> Self {
        Self::Io {
            operation,
            path: path.to_path_buf(),
            source,
        }
    }
}
