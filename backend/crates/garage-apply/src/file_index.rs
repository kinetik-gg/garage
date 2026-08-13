//! `apply_file_index()`: start, stop, or immediately refresh the background filename index.
//!
//! Disabling stops and disables the unit in one call and returns -- there is nothing further
//! to converge. Enabling is `enable` then `restart`, deliberately not `start`: frequency,
//! depth and roots are read at process startup, so a preference change has to build its first
//! new snapshot now rather than waiting for the old interval to expire, which only a restart
//! guarantees.
//!
//! Every step goes through the generic route runner ([`crate::route`]'s `run_or_raise()`
//! equivalent), so a `systemctl` failure at any of the three points is reported with a
//! message naming what specifically could not be done, rather than a bare non-zero exit.

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// Enable, disable or restart the background file index unit to match `[indexing]`.
///
/// # Errors
///
/// Always [`ApplyError::PortPending`] until Phase 3 replaces this stub.
pub(crate) fn apply_file_index(_cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    Err(ApplyError::PortPending("apply_file_index"))
}
