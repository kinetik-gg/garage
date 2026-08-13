//! The write boundary: apply a fully rendered plan and update its audit state.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{symlink, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;
use serde::Serialize;

use crate::error::ReconcileError;
use crate::ledger::{log_path, Ledger, LedgerEntry};
use crate::path::lexists;
use crate::time::RunTime;
use crate::types::{Action, PlanItem};

pub(crate) fn execute(
    paths: &Paths,
    root: &Path,
    plan: &[PlanItem],
    time: &RunTime,
    ledger: &mut Ledger,
) -> Result<usize, ReconcileError> {
    let mut applied = 0;
    for item in plan {
        match item.action {
            Action::Link => link_item(paths, item)?,
            Action::Relink => relink_item(paths, item)?,
            Action::BackupAndLink => backup_item(paths, item)?,
            Action::Prune => prune_item(paths, root, item, time, ledger)?,
        }
        if item.action != Action::Prune {
            record_link(paths, item, time, ledger)?;
        }
        applied += 1;
    }
    Ok(applied)
}

fn link_item(paths: &Paths, item: &PlanItem) -> Result<(), ReconcileError> {
    let target = paths.home.join(&item.path);
    guard_ancestors(&paths.home, &target)?;
    let source = source(item)?;
    create_parent(&target)?;
    symlink(&source, &target).map_err(|error| ReconcileError::io("create link", &target, error))
}

fn relink_item(paths: &Paths, item: &PlanItem) -> Result<(), ReconcileError> {
    let target = paths.home.join(&item.path);
    guard_ancestors(&paths.home, &target)?;
    let metadata = fs::symlink_metadata(&target)
        .map_err(|error| ReconcileError::io("inspect old link", &target, error))?;
    if !metadata.file_type().is_symlink() {
        return Err(invalid(item, "relink target stopped being a symlink"));
    }
    fs::remove_file(&target)
        .map_err(|error| ReconcileError::io("remove old link", &target, error))?;
    let source = source(item)?;
    symlink(&source, &target).map_err(|error| ReconcileError::io("create link", &target, error))
}

fn backup_item(paths: &Paths, item: &PlanItem) -> Result<(), ReconcileError> {
    let target = paths.home.join(&item.path);
    guard_ancestors(&paths.home, &target)?;
    let backup = item
        .backup
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| invalid(item, "backup-and-link action has no backup path"))?;
    if lexists(&backup) {
        let error = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "backup now exists");
        return Err(ReconcileError::io("reserve backup", &backup, error));
    }
    create_parent(&backup)?;
    fs::rename(&target, &backup)
        .map_err(|error| ReconcileError::io("move path to backup", &target, error))?;
    let source = source(item)?;
    symlink(&source, &target).map_err(|error| ReconcileError::io("create link", &target, error))
}

fn prune_item(
    paths: &Paths,
    root: &Path,
    item: &PlanItem,
    time: &RunTime,
    ledger: &mut Ledger,
) -> Result<(), ReconcileError> {
    let target = paths.home.join(&item.path);
    let checkout_link = target.is_symlink() && garage_core::stow::points_into(&target, root);
    if !ledger.contains(&item.path) && !checkout_link {
        return Err(invalid(item, "prune guard changed before removal"));
    }
    // Append and sync first. A filesystem removal and an append cannot be one atomic
    // transaction; write-ahead ordering admits a harmless attempted-removal line if the
    // subsequent unlink fails, while the opposite order permits the forbidden state: a
    // deletion with no durable audit line at all.
    append_log(paths, item, &time.timestamp)?;
    remove_path(&target)?;
    ledger.remove(&item.path);
    ledger.write(paths)
}

fn record_link(
    paths: &Paths,
    item: &PlanItem,
    time: &RunTime,
    ledger: &mut Ledger,
) -> Result<(), ReconcileError> {
    ledger.record(LedgerEntry {
        path: item.path.clone(),
        kind: item.kind.clone(),
        owner: item.owner.clone(),
        timestamp: time.timestamp.clone(),
    });
    ledger.write(paths)
}

fn append_log(paths: &Paths, item: &PlanItem, timestamp: &str) -> Result<(), ReconcileError> {
    let path = log_path(paths);
    create_parent(&path)?;
    let mut sink = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| ReconcileError::io("open reconcile log", &path, error))?;
    let line = serde_json::to_string(&LogEntry {
        path: &item.path,
        reason: &item.reason,
        timestamp,
    })?;
    writeln!(sink, "{line}")
        .and_then(|()| sink.flush())
        .and_then(|()| sink.sync_data())
        .map_err(|error| ReconcileError::io("append reconcile log", &path, error))
}

fn remove_path(path: &Path) -> Result<(), ReconcileError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ReconcileError::io("inspect prune target", path, error))?;
    let result = if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    };
    result.map_err(|error| ReconcileError::io("remove obsolete path", path, error))
}

fn guard_ancestors(home: &Path, target: &Path) -> Result<(), ReconcileError> {
    let mut current = target.parent();
    while let Some(parent) = current {
        if parent == home {
            break;
        }
        if parent.is_symlink() {
            return Err(ReconcileError::SymlinkAncestor {
                path: target.to_path_buf(),
                parent: parent.to_path_buf(),
            });
        }
        current = parent.parent();
    }
    Ok(())
}

fn create_parent(path: &Path) -> Result<(), ReconcileError> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| ReconcileError::io("create parent directory", parent, error))
}

fn source(item: &PlanItem) -> Result<PathBuf, ReconcileError> {
    item.source
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| invalid(item, "link action has no checkout source"))
}

fn invalid(item: &PlanItem, detail: &'static str) -> ReconcileError {
    ReconcileError::InvalidPlan {
        path: item.path.clone(),
        detail,
    }
}

#[derive(Serialize)]
struct LogEntry<'a> {
    path: &'a str,
    reason: &'a str,
    timestamp: &'a str,
}
