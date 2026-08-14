//! The shipped machine-migration registry.

use super::Migration;

/// No one-way transformation qualifies yet: this empty registry is the mechanism waiting
/// for its first customer. A future entry belongs here only when it removes legacy machine
/// state a fresh install would never have and the current checkout cannot derive or converge;
/// ordinary desired state remains bootstrap or `garage reconcile` work.
#[allow(
    dead_code,
    reason = "the mechanism deliberately precedes its first migration and CLI caller"
)]
pub(crate) const REGISTRY: &[Migration] = &[];
