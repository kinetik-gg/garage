//! `UpdateLock` -- one update at a time, without making a second invocation wait.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use garage_core::time::{local_iso8601, now_seconds};
use rustix::fs::{flock, FlockOperation};
use rustix::io::Errno;
use thiserror::Error;

/// Why the update lock could not be taken. Operating-system failures carry the lock file's
/// own path, because that is the path the operation refused.
#[derive(Debug, Error)]
pub enum UpdateLockError {
    /// The directory the lock file belongs in could not be created.
    #[error("{}: could not create the directory it belongs in: {source}", path.display())]
    Parents {
        /// The lock file.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// The lock file could not be opened.
    #[error("{}: could not be opened: {source}", path.display())]
    Open {
        /// The lock file.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// `flock` refused for a reason other than contention.
    #[error("{}: could not be locked: {source}", path.display())]
    Flock {
        /// The lock file.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// The start stamp could not be written after the lock was taken.
    #[error("{}: could not record when the update started: {source}", path.display())]
    Stamp {
        /// The lock file.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// Another update owns the lock and left a readable start stamp.
    #[error("another update is already running (started {started})")]
    Held {
        /// Its local `HH:MM:SS` start time.
        started: String,
    },
    /// Another update owns the lock, but its start stamp is missing or malformed.
    #[error("another update is already running")]
    HeldWithoutStamp,
}

/// Proof that this process owns `update.lock`, released when this value is dropped.
///
/// The acquire is deliberately non-blocking: waiting invisibly behind a system upgrade can
/// take twenty minutes, while a refusal gives the caller a useful answer immediately. Dry
/// runs take the same lock because a plan computed against a checkout another update is
/// rewriting is not truthful.
///
/// This lock deliberately has no interaction with `PrefLock`. Bootstrap's GPU gate is the
/// documented one-writer exception that writes `preferences.toml` outside `PrefLock`, so
/// holding that lock across bootstrap would protect nothing while serialising a settings
/// slider against a system upgrade.
///
/// `garage reconcile`, including its pacman path unit, is a known unlocked gap. Reconcile is
/// convergent and backs up displaced files, so overlap is safe if noisy. The path lives in
/// [`garage_core::paths::Locks`] so wiring reconcile to the same lock later is a one-line
/// selection rather than a path migration.
///
/// Not `Clone`, not `Send`, and not `Sync`: the file descriptor is the lock and stays in the
/// stack frame that acquired it. `File`'s `Drop` closes the descriptor on every return path,
/// which releases the advisory lock without deleting its reusable file.
#[derive(Debug)]
pub(crate) struct UpdateLock {
    _file: File,
    _not_send: PhantomData<*const ()>,
}

impl UpdateLock {
    /// Take the update lock immediately and record this invocation's local start time.
    ///
    /// `LOCK_EX | LOCK_NB`: contention is an error rather than a reason to wait. A missing,
    /// unreadable, or malformed body is ignored on that refusal path so the useful primary
    /// error is never replaced by a secondary file error.
    ///
    /// # Errors
    ///
    /// [`UpdateLockError`] if the directory or file cannot be opened, `flock` fails or finds
    /// a holder, or the start stamp cannot be written after acquiring the lock.
    pub(crate) fn acquire(path: &Path) -> Result<Self, UpdateLockError> {
        Self::acquire_with_started(path, &local_iso8601(now_seconds()))
    }

    pub(super) fn acquire_with_started(
        path: &Path,
        started: &str,
    ) -> Result<Self, UpdateLockError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| UpdateLockError::Parents {
                path: path.to_path_buf(),
                source,
            })?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            // Truncating in `open()` would erase the holder's stamp before this contender
            // learns that it does not own the lock. Truncate only after `flock` succeeds.
            .truncate(false)
            .open(path)
            .map_err(|source| UpdateLockError::Open {
                path: path.to_path_buf(),
                source,
            })?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(refused) if refused == Errno::WOULDBLOCK || refused == Errno::AGAIN => {
                return Err(refusal(&mut file));
            }
            Err(source) => {
                return Err(UpdateLockError::Flock {
                    path: path.to_path_buf(),
                    source: source.into(),
                });
            }
        }
        file.set_len(0).map_err(|source| UpdateLockError::Stamp {
            path: path.to_path_buf(),
            source,
        })?;
        file.write_all(format!("{started}\n").as_bytes())
            .map_err(|source| UpdateLockError::Stamp {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(Self {
            _file: file,
            _not_send: PhantomData,
        })
    }
}

/// The contention error, optionally enriched from the holder's lock-file body.
fn refusal(file: &mut File) -> UpdateLockError {
    let mut body = String::new();
    let started = file
        .read_to_string(&mut body)
        .ok()
        .and_then(|_| started_clock(&body));
    match started {
        Some(started) => UpdateLockError::Held { started },
        None => UpdateLockError::HeldWithoutStamp,
    }
}

/// Accept exactly the shape `local_iso8601()` emits, then keep its local clock component.
fn started_clock(body: &str) -> Option<String> {
    let stamp = body.trim();
    let valid = stamp.chars().count() == 24
        && stamp
            .chars()
            .enumerate()
            .all(|(index, character)| match index {
                4 | 7 => character == '-',
                10 => character == 'T',
                13 | 16 => character == ':',
                19 => matches!(character, '+' | '-'),
                _ => character.is_ascii_digit(),
            });
    valid.then(|| stamp.chars().skip(11).take(8).collect())
}

#[cfg(test)]
mod tests {
    use super::{UpdateLock, UpdateLockError};
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
                "garage-update-lock-{label}-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn lock_path(&self) -> PathBuf {
            self.path.join("state/update.lock")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.path));
        }
    }

    fn acquire(path: &Path) -> Result<UpdateLock, UpdateLockError> {
        UpdateLock::acquire_with_started(path, "2026-08-14T09:10:11+0700")
    }

    #[test]
    fn the_lock_can_be_acquired_again_after_drop() {
        let scratch = Scratch::new("reacquire");
        let path = scratch.lock_path();
        let first = acquire(&path).unwrap();
        drop(first);
        drop(acquire(&path).unwrap());
    }

    /// As `garage-prefs/src/lock.rs` documents, `flock` belongs to the open file
    /// description: two fresh opens in this one process contend exactly as two processes do.
    #[test]
    fn a_second_open_is_refused_with_the_holders_start_time() {
        let scratch = Scratch::new("contend");
        let path = scratch.lock_path();
        let held = acquire(&path).unwrap();
        let refused = acquire(&path).unwrap_err();
        assert_eq!(
            refused.to_string(),
            "another update is already running (started 09:10:11)"
        );
        drop(held);
    }

    #[test]
    fn a_garbled_body_does_not_hide_the_contention_refusal() {
        let scratch = Scratch::new("garbled");
        let path = scratch.lock_path();
        let held = acquire(&path).unwrap();
        fs::write(&path, "not a timestamp\n").unwrap();
        let refused = acquire(&path).unwrap_err();
        assert_eq!(refused.to_string(), "another update is already running");
        drop(held);
    }
}
