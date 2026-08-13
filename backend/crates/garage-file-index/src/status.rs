//! [`status`] -- the snapshot the preferences pane polls, and [`indexing_now`], the
//! crash-proof way it learns whether a scan is in flight.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};

use rustix::fs::{flock, FlockOperation};
use rustix::io::Errno;

use crate::config::configuration;
use crate::db::connect_readonly;
use crate::error::FileIndexError;
use crate::json::Json;
use crate::paths::IndexPaths;

/// The full status record `status` answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Status {
    pub enabled: bool,
    pub activity: String,
    pub frequency_minutes: i64,
    pub max_depth: i64,
    pub directories: Vec<String>,
    pub count: i64,
    pub last_scan_epoch: i64,
    pub last_scan_duration_ms: i64,
    pub error: String,
}

impl Status {
    #[must_use]
    pub(crate) fn to_json(&self) -> Json {
        Json::Object(vec![
            ("enabled".to_string(), Json::Bool(self.enabled)),
            ("activity".to_string(), Json::str(self.activity.clone())),
            (
                "frequency_minutes".to_string(),
                Json::Int(self.frequency_minutes),
            ),
            ("max_depth".to_string(), Json::Int(self.max_depth)),
            (
                "directories".to_string(),
                Json::Array(self.directories.iter().cloned().map(Json::str).collect()),
            ),
            ("count".to_string(), Json::Int(self.count)),
            (
                "last_scan_epoch".to_string(),
                Json::Int(self.last_scan_epoch),
            ),
            (
                "last_scan_duration_ms".to_string(),
                Json::Int(self.last_scan_duration_ms),
            ),
            ("error".to_string(), Json::str(self.error.clone())),
        ])
    }
}

/// Report an in-flight scan without trusting a crash-prone state marker: take the scan lock
/// non-blocking, and whether that succeeds *is* the answer. A process that died mid-scan
/// leaves no lock behind -- `flock` is released by the kernel the moment the holding
/// process's last file descriptor closes, crash included -- so this can never report a scan
/// that is not actually running.
///
/// # Errors
///
/// [`FileIndexError`] if the lock file's directory or the file itself cannot be opened, or
/// if `flock` fails for a reason other than the lock being held elsewhere.
pub(crate) fn indexing_now(paths: &IndexPaths) -> Result<bool, FileIndexError> {
    if let Some(parent) = paths.lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let lock_file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&paths.lock_path)?;
    match flock(&lock_file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {
            flock(&lock_file, FlockOperation::Unlock)?;
            Ok(false)
        }
        Err(refused) if refused == Errno::WOULDBLOCK || refused == Errno::AGAIN => Ok(true),
        Err(source) => Err(source.into()),
    }
}

/// Build the status record: the resolved configuration, the last completed scan's own
/// bookkeeping (read from `metadata`, tolerating no database yet or one that cannot be
/// read), and a single word summarising what is happening right now.
///
/// # Errors
///
/// [`FileIndexError`] if the configuration cannot be read, the scan lock cannot be probed,
/// or a `metadata` value that should hold a decimal integer does not.
pub(crate) fn status(paths: &IndexPaths) -> Result<Status, FileIndexError> {
    let config = configuration(paths)?;
    let metadata = read_metadata(paths);
    let active = if config.enabled {
        indexing_now(paths)?
    } else {
        false
    };
    let error = metadata.get("error").cloned().unwrap_or_default();
    let last_scan_epoch = parse_metadata_int(&metadata, "last_scan_epoch")?;
    let activity = if !config.enabled {
        "disabled"
    } else if active {
        "indexing"
    } else if !error.is_empty() {
        "error"
    } else if last_scan_epoch != 0 {
        "idle"
    } else {
        "not_indexed"
    };
    Ok(Status {
        enabled: config.enabled,
        activity: activity.to_string(),
        frequency_minutes: config.frequency_minutes,
        max_depth: config.max_depth,
        directories: config
            .directories
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect(),
        count: parse_metadata_int(&metadata, "count")?,
        last_scan_epoch,
        last_scan_duration_ms: parse_metadata_int(&metadata, "last_scan_duration_ms")?,
        error,
    })
}

/// `metadata.get(key, 0)` then `int(...)` -- an absent key is `0`; a present one must parse.
fn parse_metadata_int(
    metadata: &HashMap<String, String>,
    key: &str,
) -> Result<i64, FileIndexError> {
    match metadata.get(key) {
        None => Ok(0),
        Some(value) => Ok(value.parse::<i64>()?),
    }
}

/// `dict(database.execute("SELECT key,value FROM metadata").fetchall())`, tolerating a
/// missing database entirely (empty map) and a `SQLite` failure by collapsing the whole
/// result to a single `error` entry -- matching the Python's
/// `except sqlite3.Error as error: metadata = {"error": str(error)}`, which replaces rather
/// than merges.
fn read_metadata(paths: &IndexPaths) -> HashMap<String, String> {
    if !paths.database_path.exists() {
        return HashMap::new();
    }
    match fetch_metadata(paths) {
        Ok(metadata) => metadata,
        Err(error) => HashMap::from([("error".to_string(), error.to_string())]),
    }
}

fn fetch_metadata(paths: &IndexPaths) -> Result<HashMap<String, String>, rusqlite::Error> {
    let database = connect_readonly(paths)?;
    let mut statement = database.prepare("SELECT key,value FROM metadata")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::{indexing_now, status};
    use crate::db::{connect, set_metadata};
    use crate::paths::IndexPaths;
    use crate::refresh::refresh_index;
    use rustix::fs::{flock, FlockOperation};
    use std::collections::HashMap;
    use std::fs::{self, OpenOptions};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(label: &str) -> Self {
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "garage-file-index-status-{label}-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.path));
        }
    }

    fn paths_in(scratch: &Scratch) -> IndexPaths {
        let mut env = HashMap::new();
        env.insert(
            "HOME".to_string(),
            scratch.path().to_string_lossy().into_owned(),
        );
        env.insert(
            "GARAGE_PREFERENCES".to_string(),
            scratch
                .path()
                .join("preferences.toml")
                .to_string_lossy()
                .into_owned(),
        );
        IndexPaths::from_env_map(&env)
    }

    /// Mirrors the Python's `test_status_reports_activity_and_the_last_complete_snapshot`.
    #[test]
    fn status_reports_not_indexed_then_idle_then_indexing() {
        let scratch = Scratch::new("lifecycle");
        let paths = paths_in(&scratch);
        let documents = scratch.path().join("Documents");
        fs::create_dir_all(&documents).unwrap();
        fs::write(documents.join("notes.txt"), "").unwrap();
        fs::write(
            paths.preferences_path.clone(),
            format!(
                "[indexing]\nenabled = true\ndirectories = \"{}\"\n",
                documents.display()
            ),
        )
        .unwrap();

        let before = status(&paths).unwrap();
        assert_eq!(before.activity, "not_indexed");

        refresh_index(&paths, None).unwrap();
        let after = status(&paths).unwrap();
        assert_eq!(after.activity, "idle");
        assert_eq!(after.count, 1);
        assert!(after.last_scan_epoch > 0);

        if let Some(parent) = paths.lock_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&paths.lock_path)
            .unwrap();
        flock(&lock_file, FlockOperation::LockExclusive).unwrap();
        assert_eq!(status(&paths).unwrap().activity, "indexing");
        drop(lock_file);
    }

    #[test]
    fn indexing_now_is_false_when_the_lock_is_free() {
        let scratch = Scratch::new("free-lock");
        let paths = paths_in(&scratch);
        assert!(!indexing_now(&paths).unwrap());
        // The probe must not leave the lock held.
        assert!(!indexing_now(&paths).unwrap());
    }

    #[test]
    fn status_surfaces_a_stored_error_when_present() {
        let scratch = Scratch::new("error");
        let paths = paths_in(&scratch);
        let database = connect(&paths).unwrap();
        set_metadata(&database, "error", "disk exploded").unwrap();
        drop(database);
        let result = status(&paths).unwrap();
        assert_eq!(result.activity, "error");
        assert_eq!(result.error, "disk exploded");
    }
}
