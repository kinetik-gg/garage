//! Keyboard shortcuts: parsing a combination, reading the published catalog, loading and
//! saving the user's document, guarding a candidate set, and acting on one change.
//!
//! [`parse`] turns a combination string into modifiers and a key, and derives the
//! id/canonical/signature spellings from it. [`catalog`] reads the published default set and
//! proves whether it can be trusted -- see its own doc for the fail-closed witness-line
//! contract. [`store`] loads and serialises `keybindings.toml`. [`guard`] refuses a shortcut
//! set that could not be undone from the desktop itself. [`action`] is `keybind_action()`,
//! the whole-file read-modify-write every rebind, reset or custom-shortcut edit goes through.
//!
//! None of these is a [`Route`](garage_core::schema::routes::Route) step -- keyboard shortcut
//! changes reach the session through their own `action keybind.*` command, not through
//! `apply_changed_preference()` -- so nothing here appears in [`crate::dispatch`].
//!
//! The document itself ([`Document`], [`CustomKeybind`], [`Overrides`]) is
//! `garage-render`'s, because `render_keybinds()` consumes it and this crate is the one that
//! depends on that one. See [`garage_render::keybinds`] for why the shape lives at the
//! bottom of that edge while every rule about what may go in it lives here.

pub mod action;
pub mod catalog;
pub mod guard;
pub mod parse;
pub mod store;

#[cfg(test)]
mod parity;
#[cfg(test)]
mod parity_actions;

use garage_core::fs::atomic::AtomicWriteError;
use garage_core::toml_emit::EmitError;
use garage_prefs::LockError;
use garage_render::RenderError;
use thiserror::Error;

pub use action::keybind_action;
pub use catalog::{keybind_catalog, read_keybind_catalog, resolve_keybinds, CatalogEntry};
pub use garage_render::keybinds::{CustomKeybind, Document, Overrides};
pub use guard::guard_keybinds;
pub use store::{keybindings_toml, load_keybindings};

/// The payload one `action keybind.*` call carries, as the Python's `values.get(...)` reads
/// it.
///
/// Every field is the string the JSON object held, or `None` where the key was absent. The
/// Python takes `payload.get("command") or ""` through `str()`, so a JSON *number* sitting
/// in a string field is stringified there and is `None` here: the caller that decodes the
/// envelope is the side that knows what shape arrived. Parity gap, stated plainly and
/// deliberately small -- it is reachable only from a hand-written payload, since the pane
/// sends strings, and for `keys` the two agree exactly anyway (`parse_combination` refuses a
/// non-string with the same "A shortcut needs a key" this refuses `None` with).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeybindRequest<'a> {
    /// `values["id"]`: which bind to move, reset, update or remove.
    pub id: Option<&'a str>,
    /// `values["keys"]`: the combination to move it to.
    pub keys: Option<&'a str>,
    /// `values["description"]`: what the pane shows for a custom shortcut.
    pub description: Option<&'a str>,
    /// `values["command"]`: the shell command line a custom shortcut runs.
    pub command: Option<&'a str>,
}

/// Why a shortcut change was refused, or could not be carried out.
///
/// The Python raises one type here, `SettingsError`, and its `str()` is what reaches the
/// user through the envelope -- so the *text* is the contract and every variant below is
/// named after the expression that produced it. The refusals are worth reading as a set:
/// each one is a thing the desktop would otherwise do silently and be unable to undo from
/// its own keyboard.
#[derive(Debug, Error)]
pub enum KeybindError {
    /// `parse_combination()`: nothing, or whitespace, where a combination belongs.
    #[error("A shortcut needs a key")]
    NoKey,

    /// `parse_combination()`: a `+` with nothing on one side of it.
    #[error("{0} is not a shortcut Hyprland understands")]
    NotAShortcut(String),

    /// `parse_combination()`: a leading part that is not a modifier or one of its aliases.
    #[error("{0} is not a modifier key")]
    NotAModifier(String),

    /// `parse_combination()`: a key half that is not a keysym name, `code:NN` or `mouse:NNN`.
    #[error("{0} is not a key name Hyprland understands")]
    NotAKeyName(String),

    /// `require_bindable()`: a bare key that would be swallowed in every window.
    #[error("{0} on its own would be swallowed everywhere; add SUPER, CONTROL, ALT or SHIFT")]
    Standalone(String),

    /// `custom_keybind()`: a custom shortcut with nothing to run.
    #[error("A custom shortcut needs a command to run")]
    NoCommand,

    /// `custom_keybind()`: one of the three fields past its ceiling in [`parse::keybind_limit`].
    #[error("The {field} is longer than {limit} characters")]
    TooLong {
        /// `keys`, `description` or `command`.
        field: &'static str,
        /// The ceiling that field is held to.
        limit: usize,
    },

    /// `custom_keybind()`: a control character in one of the three fields. Tab and newline
    /// are not among them -- a shell command legitimately carries both.
    #[error("The {0} contains a control character")]
    ControlCharacter(&'static str),

    /// `guard_keybinds()`: nothing has published a catalog yet, so there is nothing to check
    /// a change against.
    #[error("The shortcut list has not been published yet. Reload Hyprland and try again.")]
    NoCatalog,

    /// `guard_keybinds()`: a catalog with no rescue bind in it at all.
    #[error("config/binds.lua declares no rescue shortcut, so there would be no way back to a terminal. Refusing to change the shortcuts.")]
    NoRescue,

    /// `guard_keybinds()`: an override on an id an *unverified* catalog does not carry. Not
    /// "there is no such shortcut", because that is not what an unprovable catalog can say --
    /// see [`catalog::read_keybind_catalog`].
    #[error("The shortcut list is still being published. Try again in a moment.")]
    Unpublished,

    /// `guard_keybinds()`: an override on an id the verified catalog does not carry.
    #[error("There is no shortcut called {0}")]
    UnknownBind(String),

    /// `guard_keybinds()`: an override on a rescue bind, which `binds.lua` would ignore
    /// anyway.
    #[error("\"{0}\" cannot be changed")]
    Protected(String),

    /// `guard_keybinds()`: two binds on one combination. Every match fires, so this is both
    /// actions on every press rather than the later one winning.
    #[error("{keys} is already used by \"{holder}\"")]
    Collision {
        /// The combination both binds claim.
        keys: String,
        /// What already holds it.
        holder: String,
    },

    /// `guard_keybinds()`: a rescue bind's combination no longer resolves to that bind.
    #[error("{keys} must keep opening \"{description}\"")]
    RescueMoved {
        /// The rescue combination.
        keys: String,
        /// What it must keep opening.
        description: String,
    },

    /// `keybind_action("rebind")`: an id the catalog does not carry.
    #[error("There is no shortcut with that name")]
    NoSuchBind,

    /// `keybind_action("add")`: the ceiling on invented shortcuts.
    #[error("There is room for {0} custom shortcuts")]
    CustomFull(usize),

    /// `keybind_action("update"/"remove")`: an id no custom shortcut carries.
    #[error("There is no custom shortcut with that name")]
    NoSuchCustom,

    /// `keybind_action()`: an operation name nothing here implements.
    #[error("Unknown shortcut action: {0}")]
    UnknownOperation(String),

    /// `reload_keybinds()`: `hyprctl reload` refused. Carries its stderr, or the fallback
    /// sentence when it had nothing to say.
    #[error("{0}")]
    Reload(String),

    /// `reload_keybinds()`: the reload left the compositor with no binds at all. The
    /// defaults have already been put back and reloaded by the time this is raised.
    #[error("That left the desktop with no shortcuts at all. The defaults have been put back.")]
    NoBindsLeft,

    /// `PREFERENCES_LOCK` could not be taken.
    #[error(transparent)]
    Lock(#[from] LockError),

    /// `keybindings.toml` or `keybinds.json` could not be replaced.
    #[error(transparent)]
    Atomic(#[from] AtomicWriteError),

    /// A value could not be spelled as TOML. Unreachable for this document, whose every
    /// value is a string, and wired rather than unwrapped because an unwrap here would be a
    /// panic in a settings write.
    #[error(transparent)]
    Emit(#[from] EmitError),

    /// `render_keybinds()` could not publish the new set.
    #[error(transparent)]
    Render(#[from] RenderError),
}
