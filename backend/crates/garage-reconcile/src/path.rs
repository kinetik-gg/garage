//! Path predicates kept identical at manifest, ledger, and prune boundaries.

use std::path::{Component, Path, PathBuf};

use crate::error::ReconcileError;
use crate::types::DesiredPath;

pub(crate) fn safe_relative(raw: &str) -> Result<(), ReconcileError> {
    let path = PathBuf::from(raw);
    let safe = !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if safe {
        Ok(())
    } else {
        Err(ReconcileError::UnsafePath { path })
    }
}

pub(crate) fn lexists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

pub(crate) fn desired_contains(desired: &[DesiredPath], candidate: &str) -> bool {
    desired.iter().any(|path| {
        path.path == candidate
            || path
                .path
                .strip_suffix('/')
                .is_some_and(|directory| candidate.starts_with(&format!("{directory}/")))
    })
}
