//! Manifest-driven filesystem convergence for `garage reconcile`.
//!
//! Desired paths come from `managed-paths.list`, filtered through package ownership. A
//! `stow-tree desktop/` record expands with the exact tree walk doctor uses, then the exact
//! same five-way classifier produces the converge plan. `units.list` is parsed and carried
//! in every report, but this crate deliberately never enables or restarts units: bootstrap
//! owns service activation, while reconcile only reports what that manifest declares.
#![forbid(unsafe_code)]

mod desired;
mod error;
mod execute;
mod human;
mod ledger;
mod path;
mod plan;
mod prune;
mod run;
mod time;
mod types;

#[cfg(test)]
mod tests;

pub use desired::{desired_state, DesiredState};
pub use error::ReconcileError;
pub use human::render_human;
pub use plan::diff;
pub use run::{reconcile, reconcile_at};
pub use time::RunTime;
pub use types::{
    Action, ActualState, DesiredPath, Diff, Options, PlanItem, PruneRefusal, Report, Unit,
};
