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
}
