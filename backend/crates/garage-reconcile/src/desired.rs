//! Desired-state construction from the three settled manifests.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use garage_core::manifest::{self, ManagedPath, PathKind, UnitKind};
use garage_core::paths::Paths;

use crate::error::ReconcileError;
use crate::types::{DesiredPath, Unit};

/// Manifest entries rejected only because their named package owner is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExcludedPath {
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) owner: String,
}

/// Desired paths plus the owner-filtered records guarded prune may later consider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredState {
    /// Active paths after package-owner filtering.
    pub paths: Vec<DesiredPath>,
    /// Desired stow leaves; the subset reconcile links.
    pub(crate) stow: Vec<DesiredPath>,
    /// Present manifest records whose package owner left `packages.list`.
    pub(crate) excluded: Vec<ExcludedPath>,
    /// Unit declarations, reported only.
    pub units: Vec<Unit>,
}

/// Load and intersect managed paths, package ownership, and unit declarations.
///
/// # Errors
///
/// [`ReconcileError`] when a manifest is invalid or a managed path is unsafe.
pub fn desired_state(paths: &Paths, root: &Path) -> Result<DesiredState, ReconcileError> {
    let manifest_dir = garage_core::checkout::manifest_dir(paths, root);
    let packages = manifest::load_packages(&manifest_dir)?;
    let managed = manifest::load_managed_paths(&manifest_dir)?;
    let units = manifest::load_units(&manifest_dir)?;
    let package_names: BTreeSet<String> = packages.into_iter().map(|entry| entry.name).collect();
    let mut state = DesiredState {
        paths: Vec::new(),
        stow: Vec::new(),
        excluded: Vec::new(),
        units: units.into_iter().map(unit).collect(),
    };
    for entry in managed {
        add_entry(&mut state, root, &package_names, entry)?;
    }
    state
        .paths
        .sort_by(|left, right| left.path.cmp(&right.path));
    state.stow.sort_by(|left, right| left.path.cmp(&right.path));
    state
        .excluded
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(state)
}

fn add_entry(
    state: &mut DesiredState,
    root: &Path,
    packages: &BTreeSet<String>,
    entry: ManagedPath,
) -> Result<(), ReconcileError> {
    safe_relative(&entry.path)?;
    if let Some(owner) = entry
        .owner
        .as_ref()
        .filter(|owner| !packages.contains(*owner))
    {
        return exclude(state, root, entry.kind, &entry.path, owner);
    }
    if entry.kind == PathKind::StowTree {
        return add_stow(state, root, &entry);
    }
    state
        .paths
        .push(desired(entry.path, entry.kind, entry.owner));
    Ok(())
}

fn add_stow(
    state: &mut DesiredState,
    root: &Path,
    entry: &ManagedPath,
) -> Result<(), ReconcileError> {
    require_desktop(&entry.path)?;
    for path in garage_core::stow::managed_paths(root) {
        let item = desired(path, PathKind::StowTree, entry.owner.clone());
        state.stow.push(item.clone());
        state.paths.push(item);
    }
    Ok(())
}

fn exclude(
    state: &mut DesiredState,
    root: &Path,
    kind: PathKind,
    path: &str,
    owner: &str,
) -> Result<(), ReconcileError> {
    let expanded = if kind == PathKind::StowTree {
        require_desktop(path)?;
        garage_core::stow::managed_paths(root)
    } else {
        vec![path.to_owned()]
    };
    for path in expanded {
        state.excluded.push(ExcludedPath {
            path,
            kind: kind_name(kind).to_owned(),
            owner: owner.to_owned(),
        });
    }
    Ok(())
}

fn desired(path: String, kind: PathKind, owner: Option<String>) -> DesiredPath {
    DesiredPath {
        path,
        kind: kind_name(kind).to_owned(),
        owner,
    }
}

fn safe_relative(raw: &str) -> Result<(), ReconcileError> {
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

fn require_desktop(path: &str) -> Result<(), ReconcileError> {
    if path == "desktop/" {
        Ok(())
    } else {
        Err(ReconcileError::StowTree(path.to_owned()))
    }
}

fn kind_name(kind: PathKind) -> &'static str {
    match kind {
        PathKind::StowTree => "stow-tree",
        PathKind::Generated => "generated",
        PathKind::Artifact => "artifact",
        PathKind::Override => "override",
    }
}

fn unit(entry: garage_core::manifest::UnitEntry) -> Unit {
    Unit {
        name: entry.name,
        kind: match entry.kind {
            UnitKind::Running => "running",
            UnitKind::Oneshot => "oneshot",
        }
        .to_owned(),
    }
}
