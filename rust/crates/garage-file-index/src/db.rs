//! [`connect`] and [`connect_readonly`] -- the two ways this binary opens the `SQLite`
//! database, and [`SCHEMA`], the tables both expect to find there.

use std::fmt::Write as _;
use std::fs;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::paths::IndexPaths;

/// `files` holds one row per indexed entry, keyed by its absolute path; `files_name_fold`
/// speeds up the name-only half of search. `metadata` is a one-row-per-key table for the
/// scan's own bookkeeping ([`crate::refresh::set_metadata`]): the epoch a scan finished,
/// how long it took, how many rows it wrote, which roots it walked, and the last error, if
/// any.
pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS files (
    path TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_fold TEXT NOT NULL,
    parent TEXT NOT NULL,
    path_fold TEXT NOT NULL,
    kind TEXT NOT NULL,
    modified_ns INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS files_name_fold ON files(name_fold);
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

/// Open the database for writing, creating its directory and schema if either is missing.
///
/// `WAL` journalling and `NORMAL` synchronous, both set on every connection rather than
/// once: `PRAGMA journal_mode` and `PRAGMA synchronous` are persisted in the database file
/// itself after the first connection, so restating them here is idempotent, not redundant
/// with a one-time setup step that does not exist. A 250&nbsp;ms busy timeout, matching
/// [`connect_readonly`]'s -- see [`crate::refresh::refresh_index`] for why a short timeout
/// on both sides is the point rather than a shortcoming.
///
/// # Errors
///
/// [`rusqlite::Error`] if the directory cannot be created, the database cannot be opened, or
/// the schema cannot be applied.
pub(crate) fn connect(paths: &IndexPaths) -> Result<Connection, rusqlite::Error> {
    if let Some(parent) = paths.database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some(format!("{}: {error}", parent.display())),
            )
        })?;
    }
    let database = Connection::open(&paths.database_path)?;
    database.busy_timeout(Duration::from_millis(250))?;
    database.execute_batch("PRAGMA journal_mode=WAL")?;
    database.execute_batch("PRAGMA synchronous=NORMAL")?;
    database.execute_batch("PRAGMA busy_timeout=250")?;
    database.execute_batch(SCHEMA)?;
    Ok(database)
}

/// Open an existing snapshot without taking a writer lock -- the connection every `search`
/// and `status` call reads through, so neither ever contends with an in-progress scan.
///
/// # Errors
///
/// [`rusqlite::Error`] if the database cannot be opened read-only -- including when no
/// database file exists yet, which every caller here checks for first
/// ([`crate::search::search_index`], [`crate::status::status`]).
pub(crate) fn connect_readonly(paths: &IndexPaths) -> Result<Connection, rusqlite::Error> {
    let uri = format!("file:{}?mode=ro", percent_encode_path(&paths.database_path));
    let database = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    database.busy_timeout(Duration::from_millis(250))?;
    database.execute_batch("PRAGMA busy_timeout=250")?;
    Ok(database)
}

/// Upsert one row of `metadata` -- the Python's `set_metadata`, which always stores a
/// string: every caller here already holds one.
///
/// # Errors
///
/// [`rusqlite::Error`] if the statement fails.
pub(crate) fn set_metadata(
    database: &Connection,
    key: &str,
    value: &str,
) -> Result<(), rusqlite::Error> {
    database.execute(
        "INSERT INTO metadata(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        (key, value),
    )?;
    Ok(())
}

/// Percent-encode the handful of bytes that would otherwise be misread by `SQLite`'s own URI
/// filename parser -- space, `%`, `?`, `#`, and anything outside ASCII -- leaving the rest of
/// the path untouched. Narrower than a general RFC 3986 encoder because the input is always
/// a filesystem path rather than an arbitrary string: no scheme, no userinfo, no query
/// string of its own to keep separate from the one this function appends.
fn percent_encode_path(path: &std::path::Path) -> String {
    let mut out = String::new();
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b' ' | b'%' | b'?' | b'#' | 0x80..=0xff => {
                let _ = write!(out, "%{byte:02X}");
            }
            _ => out.push(byte as char),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{connect, connect_readonly, percent_encode_path, set_metadata};
    use crate::paths::IndexPaths;
    use std::collections::HashMap;
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
                "garage-file-index-db-{label}-{}-{serial}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(std::fs::remove_dir_all(&self.path));
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

    #[test]
    fn connect_creates_the_schema() {
        let scratch = Scratch::new("schema");
        let paths = paths_in(&scratch);
        let database = connect(&paths).unwrap();
        let count: i64 = database
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('files','metadata')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn readonly_connection_refuses_writes() {
        let scratch = Scratch::new("ro-write");
        let paths = paths_in(&scratch);
        drop(connect(&paths).unwrap());
        let database = connect_readonly(&paths).unwrap();
        let result = database.execute("DELETE FROM files", []);
        assert!(
            result.is_err(),
            "a read-only connection must refuse a write"
        );
    }

    #[test]
    fn readonly_connection_sees_committed_metadata() {
        let scratch = Scratch::new("ro-read");
        let paths = paths_in(&scratch);
        let writer = connect(&paths).unwrap();
        set_metadata(&writer, "count", "3").unwrap();
        drop(writer);
        let reader = connect_readonly(&paths).unwrap();
        let value: String = reader
            .query_row("SELECT value FROM metadata WHERE key='count'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(value, "3");
    }

    #[test]
    fn percent_encode_escapes_only_the_narrow_set() {
        assert_eq!(percent_encode_path(Path::new("/a/b")), "/a/b");
        assert_eq!(
            percent_encode_path(Path::new("/a b/c%d?e#f")),
            "/a%20b/c%25d%3Fe%23f"
        );
    }
}
