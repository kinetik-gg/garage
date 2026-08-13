//! One injectable instant for backup naming and audit timestamps.

/// Both textual forms derived from one clock read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTime {
    pub(crate) timestamp: String,
    pub(crate) backup_stamp: String,
}

impl RunTime {
    /// Read the clock once, then derive the ledger/log and backup forms from it.
    #[must_use]
    pub fn now() -> Self {
        let seconds = garage_core::time::now_seconds();
        Self {
            timestamp: garage_core::time::local_iso8601(seconds),
            backup_stamp: garage_core::time::local_backup_stamp(seconds),
        }
    }

    /// Pin both forms in scratch-tree tests without changing process-global time.
    #[must_use]
    pub fn fixed(timestamp: &str, backup_stamp: &str) -> Self {
        Self {
            timestamp: timestamp.to_owned(),
            backup_stamp: backup_stamp.to_owned(),
        }
    }
}
