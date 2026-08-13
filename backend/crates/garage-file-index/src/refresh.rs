//! [`refresh_index`] -- the whole-index rebuild that `run` and `refresh` both call.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;
use std::time::Instant;

use rusqlite::Connection;
use rustix::fs::{flock, FlockOperation};

use crate::config::{configuration, Config};
use crate::db::{self, set_metadata};
use crate::error::FileIndexError;
use crate::json::Json;
use crate::paths::IndexPaths;
use crate::scan::index_rows;

/// A completed scan's own report: how many rows it wrote, how long it took, and which
/// configured roots actually existed and were walked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefreshResult {
    pub count: i64,
    pub duration_ms: i64,
    pub roots: Vec<String>,
}

impl RefreshResult {
    /// `{"count":...,"duration_ms":...,"roots":[...],"error":""}` -- the shape
    /// `refresh_index()`'s return value takes once it reaches [`crate::json::response`],
    /// key order matching the Python dict literal.
    #[must_use]
    pub(crate) fn to_json(&self) -> Json {
        Json::Object(vec![
            ("count".to_string(), Json::Int(self.count)),
            ("duration_ms".to_string(), Json::Int(self.duration_ms)),
            (
                "roots".to_string(),
                Json::Array(self.roots.iter().cloned().map(Json::str).collect()),
            ),
            ("error".to_string(), Json::str("")),
        ])
    }
}

/// Rebuild the index under the scan lock, reading the configuration fresh if none is given.
///
/// The lock -- a plain `flock(2)` on [`IndexPaths::lock_path`], held for the whole rebuild --
/// serialises this against every other caller: the long-lived `run` service and a one-shot
/// `refresh` from the preferences pane can land at the same moment, and without it neither
/// would fail outright (`SQLite`'s own busy timeout is short, 250&nbsp;ms), they would just
/// race to `BEGIN IMMEDIATE` and one of them would see the other's half-built table. The
/// lock is advisory and per-process-independent -- `flock` released the moment this
/// function returns, by the file going out of scope -- so it costs nothing when only one
/// caller is ever running, which is the common case.
///
/// # Errors
///
/// [`FileIndexError`] if the lock file's directory or the file itself cannot be opened, if
/// the lock cannot be taken, or if the rebuild itself fails (see
/// [`refresh_index_locked`]).
pub(crate) fn refresh_index(
    paths: &IndexPaths,
    config: Option<Config>,
) -> Result<RefreshResult, FileIndexError> {
    let config = match config {
        Some(config) => config,
        None => configuration(paths)?,
    };
    if let Some(parent) = paths.lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let lock_file = open_lock_file(&paths.lock_path)?;
    flock(&lock_file, FlockOperation::LockExclusive)?;
    let result = refresh_index_locked(paths, &config);
    drop(lock_file);
    result
}

/// The rebuild itself, run with the scan lock already held: delete every row, walk every
/// configured root that still exists, insert everything found, and record the scan's own
/// bookkeeping in `metadata` -- all inside one `BEGIN IMMEDIATE` transaction, so a reader in
/// WAL mode keeps seeing the previous committed table until this one commits and a search
/// never sees a half-built index.
///
/// On any failure, the transaction is rolled back and the failure's own message is written
/// to `metadata.error` in a second, best-effort transaction -- a write that is itself
/// allowed to fail silently, since the scan has already failed and there is nothing further
/// to report it to.
///
/// # Errors
///
/// [`FileIndexError`] if the transaction cannot be started, if any row cannot be inserted,
/// or if the commit itself fails. The connection is always closed on the way out, success
/// or failure.
fn refresh_index_locked(
    paths: &IndexPaths,
    config: &Config,
) -> Result<RefreshResult, FileIndexError> {
    let started = Instant::now();
    let roots: Vec<_> = config
        .directories
        .iter()
        .filter(|root| root.is_dir())
        .cloned()
        .collect();
    let database = db::connect(paths)?;
    match rebuild(&database, &roots, config.max_depth, started) {
        Ok(result) => Ok(result),
        Err(error) => {
            database.execute_batch("ROLLBACK").ok();
            let _ = set_metadata(&database, "error", &error.to_string());
            database.execute_batch("COMMIT").ok();
            Err(error)
        }
    }
}

fn rebuild(
    database: &Connection,
    roots: &[std::path::PathBuf],
    max_depth: i64,
    started: Instant,
) -> Result<RefreshResult, FileIndexError> {
    database.execute_batch("BEGIN IMMEDIATE")?;
    database.execute("DELETE FROM files", [])?;
    let mut statement = database.prepare(
        "INSERT INTO files(path,name,name_fold,parent,path_fold,kind,modified_ns) \
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
    )?;
    let mut count: i64 = 0;
    for root in roots {
        for row in index_rows(root, max_depth) {
            statement.execute((
                &row.path,
                &row.name,
                &row.name_fold,
                &row.parent,
                &row.path_fold,
                row.kind.as_str(),
                row.modified_ns,
            ))?;
            count += 1;
        }
    }
    drop(statement);
    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let root_strings: Vec<String> = roots
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect();
    set_metadata(database, "last_scan_epoch", &epoch_now().to_string())?;
    set_metadata(database, "last_scan_duration_ms", &duration_ms.to_string())?;
    set_metadata(database, "count", &count.to_string())?;
    set_metadata(database, "roots", &python_default_json_array(&root_strings))?;
    set_metadata(database, "error", "")?;
    database.execute_batch("COMMIT")?;
    Ok(RefreshResult {
        count,
        duration_ms,
        roots: root_strings,
    })
}

/// `json.dumps([...])` with **no** overrides -- `CPython`'s defaults, `ensure_ascii=True`
/// and `separators=(", ", ": ")` -- for the one metadata value the Python writes through a
/// bare `json.dumps` rather than through `response()`'s compact, non-ASCII-safe form. Kept
/// apart from [`crate::json`], whose whole contract is the opposite of this one call: this
/// `roots` bookkeeping value is never read back by any caller ([`crate::status::status`]
/// does not surface it), so getting its exact spelling right costs little and matters only
/// if something dumps the `metadata` table directly.
fn python_default_json_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push('"');
        for ch in item.chars() {
            let code = ch as u32;
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ if (0x20..=0x7e).contains(&code) => out.push(ch),
                _ if code <= 0xffff => {
                    use std::fmt::Write as _;
                    let _ = write!(out, "\\u{code:04x}");
                }
                _ => {
                    use std::fmt::Write as _;
                    let value = code - 0x1_0000;
                    let high = 0xd800 + (value >> 10);
                    let low = 0xdc00 + (value & 0x3ff);
                    let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
                }
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

/// Seconds since the epoch, matching `int(time.time())`.
fn epoch_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Open the scan lock file fresh -- append + create, matching the Python's `"a+"`, never
/// read from or written to. See `garage_prefs::lock`'s `open_lock_file` for why a fresh
/// open on every call is load-bearing rather than incidental: `flock` belongs to the open
/// file description, and a cached descriptor would silently stop contending with itself.
fn open_lock_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::{refresh_index, RefreshResult};
    use crate::config::Config;
    use crate::paths::IndexPaths;
    use std::collections::HashMap;
    use std::fs;
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
                "garage-file-index-refresh-{label}-{}-{serial}",
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
        IndexPaths::from_env_map(&env)
    }

    fn config_for(directories: Vec<PathBuf>) -> Config {
        Config {
            enabled: true,
            frequency_minutes: 5,
            max_depth: 8,
            directories,
        }
    }

    #[test]
    fn refresh_replaces_the_index_and_reports_a_count() {
        let scratch = Scratch::new("basic");
        let paths = paths_in(&scratch);
        let documents = scratch.path().join("Documents");
        let nested = documents.join("Clients/Kinetik");
        fs::create_dir_all(&nested).unwrap();
        fs::write(documents.join("Budget 2026.ods"), "numbers").unwrap();
        fs::write(nested.join("Launch Notes.md"), "notes").unwrap();

        let first = refresh_index(&paths, Some(config_for(vec![documents.clone()]))).unwrap();
        // The root itself is never yielded, only its contents: Budget 2026.ods, Clients,
        // Kinetik, Launch Notes.md = 4 entries.
        assert_eq!(first.count, 4);
        assert_eq!(first.roots, vec![documents.to_string_lossy().into_owned()]);

        fs::remove_file(documents.join("Budget 2026.ods")).unwrap();
        let second = refresh_index(&paths, Some(config_for(vec![documents]))).unwrap();
        assert_eq!(second.count, 3);
    }

    #[test]
    fn a_nonexistent_root_is_skipped_without_error() {
        let scratch = Scratch::new("missing-root");
        let paths = paths_in(&scratch);
        let missing = scratch.path().join("Nonexistent");
        let result = refresh_index(&paths, Some(config_for(vec![missing]))).unwrap();
        assert_eq!(result.count, 0);
        assert!(result.roots.is_empty());
    }

    #[test]
    fn to_json_matches_the_python_dict_key_order() {
        let result = RefreshResult {
            count: 2,
            duration_ms: 7,
            roots: vec!["/home/tester/Documents".to_string()],
        };
        assert_eq!(
            result.to_json().dump(),
            "{\"count\":2,\"duration_ms\":7,\"roots\":[\"/home/tester/Documents\"],\"error\":\"\"}"
        );
    }
}
