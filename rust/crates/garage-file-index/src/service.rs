//! [`run_service`] -- the `run` command's whole lifetime: refresh, sleep, repeat, until
//! `SIGTERM`/`SIGINT` or the `[indexing]` section turns itself off.
//!
//! **Dependency note, as flagged in the port brief:** catching a signal in pure `std` is not
//! possible without `unsafe` `sigaction` plumbing, which this workspace's
//! `unsafe_code = "forbid"` lint (crate-level, inherited from the workspace) rules out for
//! code written here. `signal-hook` is added to this crate only (not the workspace), with
//! default features off and just `"iterator"` enabled -- the minimum for
//! [`signal_hook::iterator::Signals`], which runs the classic self-pipe trick internally so
//! that no signal handler in this crate ever touches memory outside what is async-signal-safe.
//! `rustix` was checked first, per the brief, and does not offer signal *registration* (only
//! `kill`-adjacent process control), so it cannot cover this.
//!
//! The wait itself is `std`: a `Mutex<bool>` + `Condvar`, standing in for the Python's
//! `threading.Event`. A background thread blocks on `Signals::forever()` and, the moment a
//! `SIGTERM` or `SIGINT` arrives, sets the flag and calls `notify_all` -- so a scan sleeping
//! on `Condvar::wait_timeout` wakes immediately, the same way the Python's `stopped.wait()`
//! is interrupted the instant its signal handler calls `stopped.set()`.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::config::configuration;
use crate::error::FileIndexError;
use crate::paths::IndexPaths;
use crate::refresh::refresh_index;

/// A `threading.Event`-alike: a flag a waiter can block on, set from another thread with an
/// immediate wakeup.
struct Stopper {
    state: Mutex<bool>,
    woken: Condvar,
}

impl Stopper {
    fn new() -> Self {
        Self {
            state: Mutex::new(false),
            woken: Condvar::new(),
        }
    }

    fn set(&self) {
        #[allow(clippy::unwrap_used)] // Poisoning would mean a waiter panicked mid-hold, which
        // never happens here: nothing between lock and unlock can panic.
        let mut flag = self.state.lock().unwrap();
        *flag = true;
        self.woken.notify_all();
    }

    fn is_set(&self) -> bool {
        #[allow(clippy::unwrap_used)]
        {
            *self.state.lock().unwrap()
        }
    }

    /// Block for up to `duration`, waking early the moment [`Stopper::set`] is called.
    fn wait_timeout(&self, duration: Duration) {
        #[allow(clippy::unwrap_used)]
        let flag = self.state.lock().unwrap();
        #[allow(clippy::unwrap_used)]
        let _ = self
            .woken
            .wait_timeout_while(flag, duration, |stopped| !*stopped)
            .unwrap();
    }
}

/// Run the background service: refresh, report, sleep for `frequency_minutes`, repeat,
/// until stopped or disabled.
///
/// Every iteration reads the configuration fresh, so a preference change takes effect on
/// the next wake without a restart -- and if that reread finds indexing disabled, the loop
/// returns `0` immediately rather than sleeping first, matching the Python's
/// `if not config["enabled"]: return 0` ahead of the refresh.
///
/// A refresh failure (I/O or `SQLite`) is logged to stderr and the loop continues to its
/// next sleep -- one bad scan does not end the service, matching the Python's
/// `except (OSError, sqlite3.Error)` around the refresh call specifically. Reading the
/// configuration itself is *not* guarded the same way, in either language: a failure there
/// propagates out of the whole loop, which in the Python unwinds `run_service()` into
/// `main()`'s own outer `except`, printing an error envelope and exiting 1 even for `run`.
/// The `?` below is that same propagation.
///
/// # Errors
///
/// [`FileIndexError`] if the signal watcher cannot be installed (a self-pipe under extreme
/// resource exhaustion), or if reading the `[indexing]` configuration fails for a reason
/// [`configuration`] does not already tolerate.
pub(crate) fn run_service(paths: &IndexPaths) -> Result<i32, FileIndexError> {
    let stopper = Arc::new(Stopper::new());
    let mut signals = Signals::new([SIGTERM, SIGINT]).map_err(FileIndexError::Io)?;
    let watcher_stopper = Arc::clone(&stopper);
    let handle = signals.handle();
    let watcher = std::thread::spawn(move || {
        // One signal is all that matters: the first `SIGTERM`/`SIGINT` sets the stopper and
        // this thread's work is done. `Handle::close()` (called once the main loop returns)
        // is what makes the iterator end on its own when no signal ever arrives.
        if signals.forever().next().is_some() {
            watcher_stopper.set();
        }
    });

    let result = run_loop(paths, &stopper);

    handle.close();
    let _ = watcher.join();
    result
}

fn run_loop(paths: &IndexPaths, stopper: &Stopper) -> Result<i32, FileIndexError> {
    while !stopper.is_set() {
        let config = configuration(paths)?;
        if !config.enabled {
            return Ok(0);
        }
        match refresh_index(paths, Some(config.clone())) {
            Ok(result) => {
                println!(
                    "garage-file-index: indexed {} entries in {} ms",
                    result.count, result.duration_ms
                );
            }
            Err(error) => {
                eprintln!("garage-file-index: {error}");
            }
        }
        #[allow(clippy::cast_sign_loss)] // frequency_minutes is validated to 1..=1440.
        let seconds = (config.frequency_minutes as u64).saturating_mul(60);
        stopper.wait_timeout(Duration::from_secs(seconds));
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::run_service;
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
                "garage-file-index-service-{label}-{}-{serial}",
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

    /// A disabled `[indexing]` section must make `run` return `0` on its very first
    /// iteration -- no refresh, no sleep, and (checked here by the test simply completing
    /// promptly rather than hanging) no wait on the signal watcher either.
    #[test]
    fn a_disabled_configuration_exits_zero_immediately() {
        let scratch = Scratch::new("disabled");
        let config_dir = scratch.path().join(".config/garage");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("preferences.toml"),
            "[indexing]\nenabled = false\n",
        )
        .unwrap();
        let mut env = HashMap::new();
        env.insert(
            "HOME".to_string(),
            scratch.path().to_string_lossy().into_owned(),
        );
        let paths = IndexPaths::from_env_map(&env);

        let exit_code = run_service(&paths).unwrap();
        assert_eq!(exit_code, 0);
    }
}
