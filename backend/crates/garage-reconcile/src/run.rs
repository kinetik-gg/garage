//! Prepare, optionally cross the write barrier, and return one command report.

use std::path::Path;

use garage_core::paths::Paths;

use crate::desired::desired_state;
use crate::error::ReconcileError;
use crate::execute::execute;
use crate::ledger::Ledger;
use crate::plan::diff;
use crate::prune::prune_plan;
use crate::time::RunTime;
use crate::types::{Options, Report};

/// Reconcile the checkout owning the running Garage binary.
///
/// # Errors
///
/// Checkout, manifest, ledger, or filesystem failures are returned with their paths.
pub fn reconcile(paths: &Paths, options: Options) -> Result<Report, ReconcileError> {
    let root = garage_core::checkout::checkout_root(paths)?;
    reconcile_at(paths, &root, options, &RunTime::now())
}

/// Reconcile an explicitly named checkout at an injectable instant.
///
/// This is the scratch-tree entry point. More importantly, the dry-run barrier is visible in
/// one branch: preparation performs reads only, and [`execute`] is unreachable when
/// `options.dry_run` is true.
///
/// # Errors
///
/// Manifest, ledger, or filesystem failures are returned with their paths.
pub fn reconcile_at(
    paths: &Paths,
    root: &Path,
    options: Options,
    time: &RunTime,
) -> Result<Report, ReconcileError> {
    let (mut report, mut ledger) = prepare(paths, root, options, time)?;
    if options.dry_run {
        return Ok(report);
    }
    report.applied = execute(paths, root, &report.plan, time, &mut ledger)?;
    Ok(report)
}

fn prepare(
    paths: &Paths,
    root: &Path,
    options: Options,
    time: &RunTime,
) -> Result<(Report, Ledger), ReconcileError> {
    let desired = desired_state(paths, root)?;
    let ledger = Ledger::load(paths)?;
    let (prune, refused) = if options.prune {
        prune_plan(paths, root, &desired, &ledger)?
    } else {
        (Vec::new(), Vec::new())
    };
    let mut difference = diff(paths, root, desired, &time.backup_stamp);
    difference.plan.extend(prune);
    let report = report(paths, root, options, difference, refused);
    Ok((report, ledger))
}

fn report(
    paths: &Paths,
    root: &Path,
    options: Options,
    difference: crate::types::Diff,
    refused: Vec<crate::types::PruneRefusal>,
) -> Report {
    Report {
        dry_run: options.dry_run,
        prune: options.prune,
        checkout: root.display().to_string(),
        home: paths.home.display().to_string(),
        desired: difference.desired,
        units: difference.units,
        actual: difference.actual,
        plan: difference.plan,
        refused,
        applied: 0,
    }
}
