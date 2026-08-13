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
mod plan;
mod types;

#[cfg(test)]
mod tests;

pub use desired::{desired_state, DesiredState};
pub use error::ReconcileError;
pub use plan::diff;
pub use types::{Action, ActualState, DesiredPath, Diff, PlanItem, Unit};
