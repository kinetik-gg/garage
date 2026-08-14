//! `ApplyError`: what an applier can fail with.

use garage_core::schema::Section;
use thiserror::Error;

/// Why an apply step could not complete.
///
/// The scaffold variant this enum carried through the port is gone: every function named in
/// [`crate::dispatch`] has a real body, and every variant below is a failure a real
/// `hyprctl`, `gsettings`, `systemctl` or filesystem call can actually produce.
///
/// Two shapes, and the difference is who owns the words. `#[error(transparent)]` variants
/// defer to the layer that raised them, because that layer is where the Python spells the
/// message; the rest carry a `String` this crate built, because the Python builds it at the
/// site too. Either way the text *is* the contract -- `main()` prints `str(error)` into the
/// envelope's `error` field and nothing matches on a type.
#[derive(Debug, Error)]
pub enum ApplyError {
    /// A renderer an applier ran first failed. Most appliers are a render followed by a push,
    /// and the render half's failures are already modelled -- re-describing them here would
    /// be a second copy that could disagree with the first.
    #[error(transparent)]
    Render(#[from] garage_render::error::RenderError),

    /// A display layout the apply path refuses, in the Python's own words:
    /// `"At least one display must remain enabled"`, `"At least one display must show the
    /// desktop rather than mirror another"`, `"{a} overlaps {b}; drag their edges apart
    /// before applying"`, `"Display layout contains gaps; connect every monitor
    /// edge-to-edge"`, `"Display confirmation token expired"`, and the two
    /// `Unsupported TOML value` refusals `layout_toml()` can raise through its emitter.
    ///
    /// Every one of them is a `SettingsError` there, whose `str()` is the envelope's `error`
    /// field, so the text is the whole contract.
    #[error("{0}")]
    Layout(String),

    /// A mirror `apply_display_layout()` has to refuse rather than quietly drop -- Hyprland's
    /// `setMirror` would log and ignore it, leaving the screen disagreeing with the file.
    /// Text owned by [`garage_render::displays::MirrorRefusal`].
    #[error(transparent)]
    Mirror(#[from] garage_render::displays::MirrorRefusal),

    /// `int()` or `float()` refused a value in a display record. Text owned by
    /// [`garage_render::displays::NumberError`].
    #[error(transparent)]
    Number(#[from] garage_render::displays::NumberError),

    /// The TOML emitter refused a value on the way into `displays.toml`.
    #[error(transparent)]
    Emit(#[from] garage_core::toml_emit::EmitError),

    /// A marker file under `~/.local/state/garage/generated` could not be written.
    ///
    /// Its own variant rather than folded into [`ApplyError::Io`] because a marker write is
    /// never a plain `write()`: `write_marker()` truncates in place so the inode Quickshell
    /// is watching survives, and refuses to follow a symlink out of the state tree. Both
    /// refusals are the writer's, and its text is the one that says which.
    #[error(transparent)]
    Marker(#[from] garage_core::fs::marker::MarkerWriteError),

    /// A file this crate rewrites whole -- `displays.toml`, the pending-transaction file --
    /// could not be replaced.
    #[error(transparent)]
    Atomic(#[from] garage_core::fs::atomic::AtomicWriteError),

    /// The display transaction lock could not be taken. A different lock from the one
    /// [`ApplyError::Lock`] carries: `DISPLAY_LOCK` serialises `display_finish()` against
    /// `initialize_display_config()`, and has nothing to do with `PREFERENCES_LOCK`.
    #[error(transparent)]
    DisplayLock(#[from] crate::displays::transaction::DisplayLockError),

    /// The pending-transaction file could not be read as JSON -- `json.loads()` raising
    /// `JSONDecodeError`, which `main()` catches beside `SettingsError`.
    ///
    /// **Parity gap, stated plainly:** the shape is the Python's and the wording is
    /// `serde_json`'s. Nothing matches on either; both are printed once.
    #[error("{0}")]
    Json(#[from] serde_json::Error),

    /// The watchdog could not be started, or an I/O call outside the writers above failed.
    /// The Python's bare `OSError`, which `main()` catches beside `SettingsError`.
    #[error("{0}")]
    Io(String),

    /// `SettingsError(...)` raised by one of the four plain commands -- `doctor`, `migrate`,
    /// `repair`, `update`.
    ///
    /// The text is the whole contract, exactly as it is for the JSON envelope: the Python's
    /// `main()` catches `(SettingsError, OSError, ValueError)` around those three and prints
    /// `garage {command}: {error}` on stderr, so what carries is `str(error)` and nothing
    /// about the type. Every message this variant holds is spelled out at the site that
    /// raises it, because that is where the Python spells it.
    #[error("{0}")]
    Settings(String),

    /// Layer 2 could not be read or written. `repair --reset` is the caller: its confirming
    /// load and its `PrefLock` acquire both fail this way.
    #[error(transparent)]
    Prefs(#[from] garage_prefs::PrefsError),

    /// `PREFERENCES_LOCK` could not be taken -- `repair --reset`'s one blocking acquire.
    #[error(transparent)]
    Lock(#[from] garage_prefs::LockError),

    /// `apply_changed_preference()`'s `f"Unsupported {section} preference: {key}"`, for the
    /// three sections that name the key because each of their keys routes somewhere of its
    /// own.
    #[error("Unsupported {section} preference: {key}")]
    UnsupportedPreference {
        /// The section the key claimed to be in.
        section: Section,
        /// The key on its own, without the section -- the Python's `dotted.split(".", 1)[1]`.
        key: String,
    },

    /// `apply_changed_preference()`'s `f"Unsupported preference section: {section}"`, for a
    /// section with no route of its own to fall back on.
    #[error("Unsupported preference section: {0}")]
    UnsupportedSection(Section),

    /// `set` refused the key outright, or a value the schema would not take. Text pinned by
    /// [`garage_core::schema::SetError`].
    #[error(transparent)]
    Set(#[from] garage_core::schema::SetError),

    /// A shortcut change was refused. Text owned by [`crate::keybind::KeybindError`], whose
    /// every variant is named after the Python expression that raised it.
    #[error(transparent)]
    Keybind(#[from] crate::keybind::KeybindError),

    /// A default-application change was refused: `"Unknown default application: {role}"` or
    /// `"{desktop_id} is not installed"`.
    #[error(transparent)]
    DesktopFile(#[from] crate::desktopfiles::mime::DesktopFileError),

    /// A command that was supposed to move the session refused to.
    ///
    /// `run_or_raise()`'s `SettingsError(result.stderr.strip() or message)`: the command's own
    /// complaint when it had one, and the step's message when it did not. Both halves reach
    /// the user's stderr verbatim, which is why the choice between them is made here rather
    /// than by whoever prints it.
    #[error("{0}")]
    Signal(String),
}
