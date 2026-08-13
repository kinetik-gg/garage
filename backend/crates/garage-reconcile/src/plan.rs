//! Pure desired/actual diffing; this module performs no writes.

use std::path::{Path, PathBuf};

use garage_core::paths::Paths;
use garage_core::stow::StowOutcome;

use crate::desired::DesiredState;
use crate::types::{Action, ActualState, Diff, PlanItem};

/// Build the complete converge plan without changing the filesystem.
#[must_use]
pub fn diff(paths: &Paths, root: &Path, desired: DesiredState, stamp: &str) -> Diff {
    let classified: Vec<_> = desired
        .stow
        .iter()
        .map(|path| {
            (
                path,
                garage_core::stow::classify(root, &paths.home, &path.path),
            )
        })
        .collect();
    let plain_paths: Vec<&str> = classified
        .iter()
        .filter_map(|(path, state)| {
            matches!(state, StowOutcome::Plain).then_some(path.path.as_str())
        })
        .collect();
    let backup_root = backup_root(&paths.home, stamp, &plain_paths);
    let mut actual = ActualState::default();
    let plan = classified
        .iter()
        .filter_map(|(path, state)| plan_one(root, path, state, &backup_root, &mut actual))
        .collect();
    Diff {
        desired: desired.paths,
        units: desired.units,
        actual,
        plan,
    }
}

fn plan_one(
    root: &Path,
    path: &crate::types::DesiredPath,
    state: &StowOutcome,
    backup_root: &Path,
    actual: &mut ActualState,
) -> Option<PlanItem> {
    let (action, reason, backup) = match state {
        StowOutcome::Linked => {
            actual.linked += 1;
            return None;
        }
        StowOutcome::Other { checkout: _ } => {
            actual.other += 1;
            (Action::Relink, "linked to another checkout", None)
        }
        StowOutcome::Broken => {
            actual.broken += 1;
            (Action::Relink, "broken or unrelated link", None)
        }
        StowOutcome::Plain => {
            actual.plain += 1;
            let backup = backup_root.join(&path.path).display().to_string();
            (
                Action::BackupAndLink,
                "plain path blocks the link",
                Some(backup),
            )
        }
        StowOutcome::Missing => {
            actual.missing += 1;
            (Action::Link, "missing", None)
        }
    };
    Some(PlanItem {
        action,
        path: path.path.clone(),
        reason: reason.to_owned(),
        source: Some(root.join("desktop").join(&path.path).display().to_string()),
        backup,
        kind: path.kind.clone(),
        owner: path.owner.clone(),
    })
}

fn backup_root(home: &Path, stamp: &str, paths: &[&str]) -> PathBuf {
    let base = home.join(".garage-backup");
    let mut suffix = 1_u64;
    loop {
        let name = if suffix == 1 {
            stamp.to_owned()
        } else {
            format!("{stamp}-{suffix}")
        };
        let candidate = base.join(name);
        let collision = paths
            .iter()
            .any(|relative| lexists(&candidate.join(relative)));
        if !collision {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

fn lexists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}
