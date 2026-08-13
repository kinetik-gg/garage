//! Obsolete-path discovery and the non-negotiable two-authority prune guard.

use std::collections::BTreeMap;
use std::path::Path;

use garage_core::paths::Paths;

use crate::desired::DesiredState;
use crate::error::ReconcileError;
use crate::ledger::Ledger;
use crate::path::{desired_contains, lexists, safe_relative};
use crate::types::{Action, PlanItem, PruneRefusal};

const GUARD_REFUSAL: &str =
    "not in the install ledger and not a symlink resolving into this checkout";

#[derive(Debug, Clone)]
struct Candidate {
    path: String,
    kind: String,
    owner: Option<String>,
    reason: String,
}

/// Build guarded removals and explicit refusals for present paths outside desired state.
///
/// The guard is intentionally redundant. A manifest diff alone is never deletion authority:
/// a typo, an owner renamed before `packages.list` is updated, or a newly ignored directory
/// can all make a valuable user file look absent. The durable ledger proves that reconcile
/// itself previously claimed the exact HOME-relative name. Independently, a symlink whose
/// target resolves into this exact checkout proves its ownership from the filesystem, which
/// lets machines installed before the ledger existed clean up safely. A candidate satisfying
/// neither proof is reported and left byte-for-byte alone. This check is repeated by the
/// executor immediately before removal; the plan is evidence, not a capability that survives
/// an intervening filesystem change.
pub(crate) fn prune_plan(
    paths: &Paths,
    root: &Path,
    desired: &DesiredState,
    ledger: &Ledger,
) -> Result<(Vec<PlanItem>, Vec<PruneRefusal>), ReconcileError> {
    let mut candidates = BTreeMap::new();
    add_ledger(paths, desired, ledger, &mut candidates);
    add_checkout_links(paths, root, desired, &mut candidates)?;
    add_excluded(paths, desired, &mut candidates);
    let mut removals = Vec::new();
    let mut refused = Vec::new();
    for candidate in candidates.into_values() {
        match admit(paths, root, ledger, candidate) {
            Admission::Removal(item) => removals.push(item),
            Admission::Refusal(item) => refused.push(item),
        }
    }
    Ok((removals, refused))
}

fn add_ledger(
    paths: &Paths,
    desired: &DesiredState,
    ledger: &Ledger,
    candidates: &mut BTreeMap<String, Candidate>,
) {
    for entry in &ledger.paths {
        let obsolete = !desired_contains(&desired.paths, &entry.path);
        if obsolete && lexists(&paths.home.join(&entry.path)) {
            let reason = ledger_reason(desired, entry.owner.as_deref());
            candidates.insert(
                entry.path.clone(),
                Candidate {
                    path: entry.path.clone(),
                    kind: entry.kind.clone(),
                    owner: entry.owner.clone(),
                    reason,
                },
            );
        }
    }
}

fn add_checkout_links(
    paths: &Paths,
    root: &Path,
    desired: &DesiredState,
    candidates: &mut BTreeMap<String, Candidate>,
) -> Result<(), ReconcileError> {
    for target in garage_core::stow::checkout_links(paths, root) {
        let Ok(relative) = target.strip_prefix(&paths.home) else {
            continue;
        };
        let relative = relative.display().to_string();
        safe_relative(&relative)?;
        if !desired_contains(&desired.paths, &relative) {
            candidates.entry(relative.clone()).or_insert(Candidate {
                path: relative,
                kind: "stow-tree".to_owned(),
                owner: None,
                reason: "path removed from managed-paths.list".to_owned(),
            });
        }
    }
    Ok(())
}

fn add_excluded(
    paths: &Paths,
    desired: &DesiredState,
    candidates: &mut BTreeMap<String, Candidate>,
) {
    for entry in &desired.excluded {
        if lexists(&paths.home.join(&entry.path)) {
            candidates.insert(
                entry.path.clone(),
                Candidate {
                    path: entry.path.clone(),
                    kind: entry.kind.clone(),
                    owner: Some(entry.owner.clone()),
                    reason: format!("owner {} removed from packages.list", entry.owner),
                },
            );
        }
    }
}

fn admit(paths: &Paths, root: &Path, ledger: &Ledger, candidate: Candidate) -> Admission {
    let target = paths.home.join(&candidate.path);
    let checkout_link = target.is_symlink() && garage_core::stow::points_into(&target, root);
    if ledger.contains(&candidate.path) || checkout_link {
        Admission::Removal(PlanItem {
            action: Action::Prune,
            path: candidate.path,
            reason: candidate.reason,
            source: None,
            backup: None,
            kind: candidate.kind,
            owner: candidate.owner,
        })
    } else {
        Admission::Refusal(PruneRefusal {
            path: candidate.path,
            reason: candidate.reason,
            guard: GUARD_REFUSAL.to_owned(),
        })
    }
}

enum Admission {
    Removal(PlanItem),
    Refusal(PruneRefusal),
}

fn ledger_reason(desired: &DesiredState, owner: Option<&str>) -> String {
    match owner.filter(|owner| !desired.packages.contains(*owner)) {
        Some(owner) => format!("owner {owner} removed from packages.list"),
        None => "path removed from managed-paths.list".to_owned(),
    }
}
