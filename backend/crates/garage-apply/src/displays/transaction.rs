//! `display_test()` and `display_finish()`: apply a candidate layout provisionally, and
//! confirm or revert it.
//!
//! `initialize_display_config()` lives here too: the first apply on a machine that has never
//! had a saved layout seeds `displays.toml` from what Hyprland currently reports -- the
//! arrangement the catch-all monitor rule produced -- so `workspace_outputs()` has a saved
//! layout to lead with from the very first session rather than losing a sleeping display's
//! block before the user ever opens the Displays pane. Never an overwrite: the file belongs
//! to the user from the moment it exists, and silent when there is nothing to record, since
//! `garage render` also runs where no compositor is answering.
//!
//! # The fifteen-second watchdog invariant
//!
//! `display_test()` applies a candidate layout at once -- so the user can see it -- but does
//! not write it to `displays.toml` until it is confirmed. A detached watchdog process is
//! spawned alongside the apply, carrying the same token the pending-transaction file is keyed
//! on, and if nobody confirms within fifteen seconds it reverts the layout to what was
//! running before the test, unattended. That is what makes it safe to test a layout that
//! might leave no working input device or no visible display at all: the machine heals itself
//! back to a known-good arrangement without anyone having to find a keyboard that still
//! works.
//!
//! The watchdog has to survive the process that started it -- `display_test()` returns a
//! token immediately, well before fifteen seconds have passed -- which is why it is spawned
//! detached rather than run to completion inline. See
//! [`Runner::spawn_self_detaching`](garage_core::traits::Runner::spawn_self_detaching) for how
//! the child reaches a session of its own without any `unsafe` in this workspace.
//!
//! `display_finish()` is idempotent against a watchdog that fires after the user already
//! confirmed: both paths take `DISPLAY_LOCK`, and whichever runs second finds the pending
//! transaction already gone and returns without doing anything.
//!
//! # `expires` is written and never read
//!
//! The pending record carries `expires` -- `time.time() + 15` -- and nothing in the backend
//! ever looks at it. It is a record of when the watchdog was *due*, not a deadline anything
//! enforces: `display_finish()` honours a pending transaction however old it is, and there is
//! no sweeper anywhere that expires one. So the watchdog is not a belt-and-braces alongside
//! some expiry check; it is the only thing that ends an unconfirmed transaction. A watchdog
//! that never ran leaves the tested layout on screen and the pending file on disk until the
//! next `display-test` overwrites it -- which is exactly why the spawn's session handling is
//! worth getting right rather than approximating.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use garage_core::fs::atomic::atomic_write;
use garage_core::pyrepr::py_float_repr;
use garage_render::displays::{render_displays, DisplayEntry, DisplayLayout, LayoutValue};
use rustix::fs::{flock, FlockOperation};
use serde_json::Value;
use thiserror::Error;

use crate::cx::SessionCx;
use crate::displays::apply::apply_display_layout;
use crate::displays::config::{layout_toml, normalize_display_layout};
use crate::displays::wire::{entry_to_json, layout_from_json, to_json, write_string};
use crate::error::ApplyError;
use crate::snapshot::display::display_snapshot;

/// How long a tested layout stands unconfirmed, in seconds. The watchdog sleeps this long and
/// the pending record's `expires` is stamped this far ahead; both are the Python's `15`.
pub const CONFIRM_WINDOW: u64 = 15;

/// Why `DISPLAY_LOCK` could not be taken. Every variant carries the lock file's own path,
/// because that is the path the operating system refused.
#[derive(Debug, Error)]
pub enum DisplayLockError {
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
    /// `flock` refused.
    #[error("{}: could not be locked: {source}", path.display())]
    Flock {
        /// The lock file.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
}

/// Proof that this thread holds `DISPLAY_LOCK`, for as long as the value is alive.
///
/// A different lock from `PREFERENCES_LOCK` and held for a different reason: this one
/// serialises `display_finish()` against `initialize_display_config()`. Session start runs
/// the seeding while the Displays pane is coming up, and a layout confirmed in between must
/// not be replaced by a snapshot of the arrangement it was confirmed away from.
///
/// Not `Clone`, not `Send`, not `Sync`, and released by being dropped -- the same reasoning
/// as `garage-prefs`' `PrefLock`, which this deliberately mirrors rather than reuses: that
/// type names `paths.locks.preferences` and a second lock file is a second lock, not a second
/// caller of the first.
#[derive(Debug)]
pub struct DisplayLock {
    _file: File,
    _not_send: PhantomData<*const ()>,
}

impl DisplayLock {
    /// Take the lock, waiting for whoever holds it.
    ///
    /// # Errors
    ///
    /// [`DisplayLockError`] if the directory cannot be created, the file cannot be opened, or
    /// `flock` fails.
    pub fn acquire(path: &Path) -> Result<Self, DisplayLockError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| DisplayLockError::Parents {
                path: path.to_path_buf(),
                source,
            })?;
        }
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)
            .map_err(|source| DisplayLockError::Open {
                path: path.to_path_buf(),
                source,
            })?;
        flock(&file, FlockOperation::LockExclusive).map_err(|source| DisplayLockError::Flock {
            path: path.to_path_buf(),
            source: source.into(),
        })?;
        Ok(Self {
            _file: file,
            _not_send: PhantomData,
        })
    }
}

/// The transaction a `display-test` left behind, as `display-pending.json` holds it.
#[derive(Debug, Clone)]
pub struct Pending {
    /// The token `display-confirm` or `display-revert` has to present.
    pub token: String,
    /// The arrangement that was running before the test, to go back to.
    pub previous: DisplayLayout,
    /// The arrangement under test, to write out if it is confirmed.
    pub candidate: DisplayLayout,
}

/// `initialize_display_config()` (garage:5251-5293): record the machine's own arrangement,
/// once.
///
/// The tracked config names no connector: `config.monitors` ships a single catch-all rule and
/// every named monitor rule reaches Hyprland through the generated fragment, which is
/// rendered from `displays.toml`. Nothing writes that file but a confirmed layout from the
/// Displays pane, though, so until this the machine of an owner who never opened the pane
/// stayed on the catch-all for good -- preferred mode, auto position, scale 1 -- and
/// `workspace_outputs()` had no saved layout to lead with, which is what loses a sleeping
/// display its block.
///
/// Never an overwrite. The existence check is made twice: once before the lock, so the common
/// case costs no lock at all, and once inside it, because that is the check that is actually
/// safe against a `display_finish()` landing in between.
///
/// # Errors
///
/// [`ApplyError::DisplayLock`] if the lock cannot be taken, [`ApplyError::Number`] or
/// [`ApplyError::Emit`] if the snapshot cannot be normalised or serialised, and
/// [`ApplyError::Atomic`] if the file cannot be written.
pub fn initialize_display_config(
    cx: &SessionCx<'_>,
    primary_environment: &str,
) -> Result<(), ApplyError> {
    let path = cx.render().paths().host.displays.clone();
    if path.exists() {
        return Ok(());
    }
    let displays = display_snapshot(cx, primary_environment);
    if displays.is_empty() {
        return Ok(());
    }
    let lock = DisplayLock::acquire(&cx.render().paths().locks.display)?;
    if path.exists() {
        return Ok(());
    }
    // `display_snapshot()` always marks one display primary -- the focused one when nothing
    // has been chosen yet -- so this only misses if the list is empty, handled above.
    let layout = normalize_display_layout(&DisplayLayout {
        primary: primary_of(&displays),
        displays,
    })?;
    atomic_write(&path, &layout_toml(&layout)?)?;
    drop(lock);
    Ok(())
}

/// `next((item["output"] for item in displays if item["primary"]), "")`.
fn primary_of(displays: &[DisplayEntry]) -> String {
    displays
        .iter()
        .find(|entry| entry.get("primary").is_some_and(LayoutValue::truthy))
        .map(DisplayEntry::output)
        .unwrap_or_default()
}

/// `display_test()` (garage:5296-5310): apply a candidate layout for fifteen seconds.
///
/// The pending record is written *before* the apply, which is deliberate and is the Python's
/// order: if the apply refuses the layout, the record naming the arrangement to go back to is
/// already on disk, and the watchdog is simply never spawned to use it.
///
/// # Errors
///
/// Whatever [`apply_display_layout`] refuses, plus [`ApplyError::Atomic`] if the pending
/// record cannot be written and [`ApplyError::Io`] if the watchdog cannot be started.
pub fn display_test(
    cx: &SessionCx<'_>,
    payload: &Value,
    primary_environment: &str,
) -> Result<String, ApplyError> {
    let token = fresh_token();
    let candidate = normalize_display_layout(&layout_from_json(payload))?;
    let previous_displays = display_snapshot(cx, primary_environment);
    let previous = DisplayLayout {
        primary: primary_of(&previous_displays),
        displays: previous_displays,
    };
    atomic_write(
        &cx.render().paths().pending_display,
        &pending_json(&token, &previous, &candidate),
    )?;
    apply_display_layout(cx, &candidate)?;
    let executable = std::env::current_exe()
        .map_err(|error| ApplyError::Io(error.to_string()))?
        .to_string_lossy()
        .into_owned();
    cx.proc()
        .spawn_self_detaching(&[&executable, "_display-watchdog", &token])
        .map_err(|error| ApplyError::Io(error.detail))?;
    Ok(token)
}

/// `display_finish()` (garage:5313-5328): keep the layout under test, or put the previous one
/// back.
///
/// Returns without doing anything when there is no pending transaction, which is what makes
/// the watchdog safe to fire after a confirm that already happened.
///
/// # Errors
///
/// [`ApplyError::Layout`] for a token that does not match the pending one,
/// [`ApplyError::Json`] for a pending file that is not JSON, and whatever
/// [`apply_display_layout`] and the writers refuse.
pub fn display_finish(cx: &SessionCx<'_>, token: &str, confirm: bool) -> Result<(), ApplyError> {
    let lock = DisplayLock::acquire(&cx.render().paths().locks.display)?;
    let path = &cx.render().paths().pending_display;
    if !path.exists() {
        return Ok(());
    }
    let pending = read_pending(path)?;
    if pending.token != token {
        return Err(ApplyError::Layout(
            "Display confirmation token expired".to_owned(),
        ));
    }
    if confirm {
        atomic_write(
            &cx.render().paths().host.displays,
            &layout_toml(&pending.candidate)?,
        )?;
        // Rendered here and again inside `apply_display_layout()`. Kept, rather than tidied
        // into one call: the fragment is written twice in the Python too, and the second
        // write is what the trace records after the geometry check has passed.
        render_displays(cx.render(), &pending.candidate)?;
        apply_display_layout(cx, &pending.candidate)?;
    } else {
        apply_display_layout(cx, &pending.previous)?;
    }
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(ApplyError::Io(error.to_string())),
    }
    drop(lock);
    Ok(())
}

/// `json.loads(PENDING_DISPLAY.read_text())`, narrowed to the three fields anything reads.
///
/// `pending.get("token")` is compared against the argument as Python compares it: a record
/// with no token at all can never match, since the caller's token is a hex string.
fn read_pending(path: &Path) -> Result<Pending, ApplyError> {
    let text = fs::read_to_string(path).map_err(|error| ApplyError::Io(error.to_string()))?;
    let document: Value = serde_json::from_str(&text)?;
    Ok(Pending {
        token: document
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        previous: document
            .get("previous")
            .map(layout_from_json)
            .unwrap_or_default(),
        candidate: document
            .get("candidate")
            .map(layout_from_json)
            .unwrap_or_default(),
    })
}

/// `json.dumps({"token": ..., "previous": ..., "candidate": ..., "expires": ...})`.
///
/// **Fidelity boundary, stated plainly:** the key order, the separators and the escaping are
/// the Python's, but a `display-test` payload carrying top-level keys other than `primary`
/// and `displays` loses them here -- the port models a layout as those two fields, where the
/// Python carries the whole dict through. Nothing reads such a key: `layout_toml()` and
/// `apply_display_layout()` both look only at the two, and this file is read back by nothing
/// but [`read_pending`] one command later.
fn pending_json(token: &str, previous: &DisplayLayout, candidate: &DisplayLayout) -> String {
    let mut out = String::from("{\"token\": ");
    write_string(token, &mut out);
    out.push_str(", \"previous\": ");
    write_layout(previous, &mut out);
    out.push_str(", \"candidate\": ");
    write_layout(candidate, &mut out);
    // `time.time() + 15`: seconds since the epoch as a float, spelled by `float.__repr__`.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs_f64())
        .unwrap_or_default();
    #[allow(
        clippy::cast_precision_loss,
        reason = "the window is a whole number of seconds added to a wall clock"
    )]
    let expires = now + CONFIRM_WINDOW as f64;
    let _ = write!(out, ", \"expires\": {}}}", py_float_repr(expires));
    out
}

/// One layout as `{"displays": [...], "primary": "..."}` -- the key order `display_test()`
/// builds `previous` in, and the one a normalised candidate carries too.
fn write_layout(layout: &DisplayLayout, out: &mut String) {
    out.push_str("{\"displays\": [");
    for (at, entry) in layout.displays.iter().enumerate() {
        if at > 0 {
            out.push_str(", ");
        }
        entry_to_json(entry, out);
    }
    out.push_str("], \"primary\": ");
    to_json(&LayoutValue::Str(layout.primary.clone()), out);
    out.push('}');
}

/// `uuid.uuid4().hex`: sixteen random bytes with version 4 and the RFC 4122 variant stamped
/// in, spelled as thirty-two lowercase hex digits with no dashes.
///
/// Read from `/dev/urandom` rather than pulled from a crate. The token is a nonce that lives
/// for fifteen seconds inside one user's own state directory, and the property it needs is
/// that two overlapping tests cannot collide -- which sixteen kernel-random bytes give
/// outright. A failed read falls back to the clock, which is worse and still unique per
/// process: it is the difference between a strong nonce and a weak one, not between working
/// and not.
fn fresh_token() -> String {
    let mut bytes = [0u8; 16];
    if File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .is_err()
    {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.subsec_nanos())
            .unwrap_or_default();
        let seed = u128::from(nanos) ^ (u128::from(std::process::id()) << 64);
        bytes = seed.to_be_bytes();
    }
    if let Some(byte) = bytes.get_mut(6) {
        *byte = (*byte & 0x0f) | 0x40;
    }
    if let Some(byte) = bytes.get_mut(8) {
        *byte = (*byte & 0x3f) | 0x80;
    }
    let mut out = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
