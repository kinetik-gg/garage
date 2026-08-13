//! `PrefLock` — the advisory lock every read-modify-write of layer 2 is held across.
//!
//! One lock file, `~/.local/state/garage/preferences.lock`, and two ways to take it. The
//! difference between them is the whole reason this module exists, so it is stated once
//! here and repeated on each entry point.
//!
//! Layer 2 -- `preferences.toml`, `keybindings.toml` -- is rewritten whole by every writer.
//! The UI's sliders fire one `garage set` per wheel notch, so several writers run at once,
//! and unserialised the last one wins with a copy of the document that predates its
//! neighbours' keys. The lock is held across the *whole* read-modify-write, load through
//! apply, and never merely across the write: a stale apply landing last would leave the
//! desktop disagreeing with the file it was rendered from.
//!
//! The load path may never *wait* on that lock. `garage set lock.*` holds it across a
//! synchronous `systemctl --user restart hypridle.service`, and that unit's `ExecStartPre`
//! re-enters this binary as `garage render-idle`, which loads the preferences. A blocking
//! acquire anywhere on the load path would therefore have the setting deadlock against
//! itself: the `set` waits on `systemctl`, `systemctl` waits on `ExecStartPre`, and
//! `ExecStartPre` waits on the lock the `set` is holding. That is what
//! [`PrefLock::try_acquire`] is for, and why the one load-path caller
//! (`compact_preferences_file` in the Python) skips its work rather than blocking.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use garage_core::paths::Paths;
use rustix::fs::{flock, FlockOperation};
use rustix::io::Errno;
use thiserror::Error;

/// Why the preferences lock could not be taken. Every variant carries the lock file's own
/// path -- not the preferences file's -- because that is the path the operating system
/// refused.
#[derive(Debug, Error)]
pub enum LockError {
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
    /// `flock` refused for a reason other than the lock being held elsewhere.
    #[error("{}: could not be locked: {source}", path.display())]
    Flock {
        /// The lock file.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
}

/// Proof that this thread holds the preferences lock, for as long as the value is alive.
///
/// Holding one proves exactly one thing, and it is worth being precise about what:
/// no other holder of this lock file is between its own load of layer 2 and the write that
/// follows. It says nothing about the file's contents, and it guards nothing that does not
/// go through it -- the lock is advisory, so a process that never asks for it writes
/// whenever it likes. It is a capability token: functions that must only run inside the
/// lock take a `&PrefLock` they cannot forge, and the compiler then refuses the call that
/// forgot to take it.
///
/// # Not `Clone`, not `Copy`, not `Send`, not `Sync`
///
/// Not `Clone` or `Copy` because the token is the lock: a second copy would outlive the
/// release and go on proving something that had stopped being true. Not `Send` or `Sync` --
/// enforced by the `PhantomData<*const ()>` field -- because a `flock` belongs to the open
/// file description, not to the thread, so a token moved to another thread would let that
/// thread's writes claim a mutual exclusion nothing is enforcing between the two of them.
/// The token stays where it was taken, which is also how every caller in the Python uses
/// it: one `with` block, one stack frame, one thread.
///
/// # Release
///
/// There is no `unlock` and no `Drop` impl. `flock` is released when the last descriptor
/// for its open file description is closed, so dropping the `File` this holds *is* the
/// release, and `Drop` for `File` already does it. Nothing is deleted: the lock file is
/// left on disk for the next acquire, exactly as the Python leaves it.
///
/// # Which callers block
///
/// Five sites take it blocking, all of them writers, named here as they are named in
/// `desktop/.local/bin/garage`:
///
/// - `main()`'s `set` branch -- held across load, `set_nested`, `validate_preferences`,
///   `save_preferences` and `apply_changed_preference`. One process per wheel tick.
/// - `toggle_boolean_preference` -- the same read-modify-write, inverting one boolean.
/// - `action("glass.reset")` -- another whole-file read-modify-write; the pane can fire it
///   while a slider elsewhere in the same pane is still writing.
/// - `keybind_action` -- the same again over `keybindings.toml`; the pane can fire a rebind
///   while a slider is still landing.
/// - `repair_reset` -- back up, write a fresh file, load it back, all three under one hold.
///
/// One site takes it non-blocking: `compact_preferences_file`, which is on the load path.
/// See [`PrefLock::try_acquire`].
#[derive(Debug)]
pub struct PrefLock {
    /// The open lock file. Never read or written -- the descriptor is the whole point, and
    /// closing it on drop is what releases the lock.
    _file: File,
    /// `!Send` and `!Sync`: the descriptor must not cross threads.
    _not_send: PhantomData<*const ()>,
}

impl PrefLock {
    /// Take the lock, waiting for whoever holds it.
    ///
    /// `LOCK_EX` with no `LOCK_NB`, which is what the five writer sites listed on
    /// [`PrefLock`] use. Blocking is right for them because each is a person or a pane
    /// asking for a change: waiting for a neighbouring slider to finish is what serialises
    /// the two, and there is no work to skip. It is safe for them because none of them
    /// restarts a service whose `ExecStartPre` re-enters this binary -- the one route that
    /// does, `PREFERENCE_ROUTES["idle"]`, restarts `hypridle.service` from inside `set`,
    /// and `render-idle` is deliberately kept to the one fragment hypridle reads so that
    /// the re-entrant render never reaches a load that takes this lock.
    ///
    /// Never call this from anything a load can reach. See [`PrefLock::try_acquire`].
    ///
    /// # Errors
    ///
    /// [`LockError`] if the lock file's directory cannot be created, the file cannot be
    /// opened, or `flock` fails for any reason other than contention -- which cannot
    /// happen here, since this call waits instead.
    pub fn acquire(paths: &Paths) -> Result<Self, LockError> {
        let path = &paths.locks.preferences;
        let file = open_lock_file(path)?;
        flock(&file, FlockOperation::LockExclusive).map_err(|source| LockError::Flock {
            path: path.clone(),
            source: source.into(),
        })?;
        Ok(Self::held(file))
    }

    /// Take the lock if it is free, and report `Ok(None)` if it is not.
    ///
    /// `LOCK_EX | LOCK_NB`. `Ok(None)` means somebody else holds it and **the caller must
    /// skip its work**, not retry and not wait. This is the load path's only way in, and
    /// the reason it exists is a deadlock that is easy to reintroduce:
    ///
    /// 1. `garage set lock.timeout …` takes this lock, blocking, and holds it across its
    ///    whole read-modify-write.
    /// 2. Applying that key runs `PREFERENCE_ROUTES["idle"]`, whose second step is a
    ///    *synchronous* `systemctl --user restart hypridle.service`.
    /// 3. `hypridle.service`'s `ExecStartPre` re-enters this binary as
    ///    `garage render-idle`.
    /// 4. `render-idle` loads the preferences, and the load path reaches
    ///    `compact_preferences_file`.
    /// 5. Were that acquire blocking, it would wait on the lock held in step 1 -- which is
    ///    waiting on step 2, which is waiting on step 3. The `set` deadlocks against
    ///    itself, and the desktop's idle settings hang.
    ///
    /// Skipping is safe, which is what makes `Ok(None)` an answer rather than a failure:
    /// whoever holds the lock is a writer, and every writer emits departures only -- so the
    /// compaction the load wanted to perform is precisely what the holder is about to do
    /// anyway. The effective configuration is identical either way; only the file's shape
    /// waits for the next load.
    ///
    /// # Errors
    ///
    /// [`LockError`] if the lock file's directory cannot be created, the file cannot be
    /// opened, or `flock` fails for a reason other than contention. Callers on the load
    /// path are expected to treat all three the way the Python does -- a read-only config
    /// directory must not fail a load -- and carry on with the document they already have.
    pub fn try_acquire(paths: &Paths) -> Result<Option<Self>, LockError> {
        let path = &paths.locks.preferences;
        let file = open_lock_file(path)?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Some(Self::held(file))),
            // `EWOULDBLOCK` is `EAGAIN` on Linux; both are named so that the intent
            // survives a port to a platform where they differ.
            Err(refused) if refused == Errno::WOULDBLOCK || refused == Errno::AGAIN => Ok(None),
            Err(source) => Err(LockError::Flock {
                path: path.clone(),
                source: source.into(),
            }),
        }
    }

    /// The token, once the kernel has granted the lock.
    fn held(file: File) -> Self {
        Self {
            _file: file,
            _not_send: PhantomData,
        }
    }
}

/// Open the lock file fresh, creating its directory first.
///
/// Opened at every acquire and never cached, which is load-bearing rather than tidy.
/// `flock` belongs to the open file description: two `open()` calls in one process produce
/// two of them and contend with each other exactly as two processes would, while a cached
/// descriptor asked for the lock a second time is granted it immediately -- the kernel
/// converts a lock the caller already holds instead of waiting -- and the mutual exclusion
/// silently stops existing within the process. Every site in the Python opens the file
/// inside its own `with`, and that property is being ported along with the lock.
///
/// Append + create + read, mirroring the Python's `"a+"`. Nothing is ever read from or
/// written to it; append is simply the mode that neither truncates the file nor requires it
/// to exist.
fn open_lock_file(path: &Path) -> Result<File, LockError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| LockError::Parents {
            path: path.to_path_buf(),
            source,
        })?;
    }
    OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
        .map_err(|source| LockError::Open {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::{Paths, PrefLock};
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    /// A `HOME` of its own per test, removed when the test ends, so two tests never race
    /// over one lock file.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(label: &str) -> Self {
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "garage-prefs-{label}-{}-{serial}",
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

    fn paths_in(scratch: &Scratch) -> Paths {
        let mut env = HashMap::new();
        env.insert(
            "HOME".to_owned(),
            scratch.path().to_string_lossy().into_owned(),
        );
        Paths::from_env_map(&env)
    }

    #[test]
    fn the_lock_can_be_taken_again_once_it_is_dropped() {
        let scratch = Scratch::new("reacquire");
        let paths = paths_in(&scratch);
        let first = PrefLock::acquire(&paths).unwrap();
        drop(first);
        drop(PrefLock::acquire(&paths).unwrap());
    }

    /// Two opens in one process contend, because `flock` is per open file description and
    /// not per process. That is exactly why the lock file is opened fresh at every acquire
    /// rather than cached: a cached descriptor would be granted the lock a second time and
    /// the exclusion would quietly disappear inside the process.
    #[test]
    fn try_acquire_reports_the_lock_as_held_from_a_second_open_in_this_process() {
        let scratch = Scratch::new("contend");
        let paths = paths_in(&scratch);
        let held = PrefLock::acquire(&paths).unwrap();
        assert!(PrefLock::try_acquire(&paths).unwrap().is_none());
        drop(held);
    }

    #[test]
    fn try_acquire_succeeds_once_the_holder_is_dropped() {
        let scratch = Scratch::new("released");
        let paths = paths_in(&scratch);
        let held = PrefLock::acquire(&paths).unwrap();
        assert!(PrefLock::try_acquire(&paths).unwrap().is_none());
        drop(held);
        assert!(PrefLock::try_acquire(&paths).unwrap().is_some());
    }

    #[test]
    fn acquiring_creates_the_directory_the_lock_file_belongs_in() {
        let scratch = Scratch::new("mkdir");
        let paths = paths_in(&scratch);
        assert!(!paths.state_root.exists());
        drop(PrefLock::acquire(&paths).unwrap());
        assert!(paths.locks.preferences.exists());
    }
}
