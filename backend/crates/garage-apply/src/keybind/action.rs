//! `keybind_action()`: apply one change (rebind, reset, reset-all, add, update, remove) to
//! the shortcut set.
//!
//! Under the preferences lock, like `set` -- this is another whole-file read-modify-write,
//! and the pane can fire a rebind while a slider elsewhere is still landing on
//! `preferences.toml`. It is the fourth of the five blocking lock sites named on
//! [`PrefLock`], and the only one whose file is `keybindings.toml` rather than
//! `preferences.toml`: one lock serialises every layer-2 read-modify-write, whatever it is
//! rewriting, because the pane fires them at each other.
//!
//! A rebind back to a shortcut's own default is a removal, not an override that happens to
//! agree with the default: keeping it as an override would leave the pane showing the
//! shortcut as changed, and would pin it if the default itself ever moved in a later release.
//!
//! [`reload_keybinds`] is the signal half: reload, then prove the desktop still has keys.
//! Nothing else would notice an empty bind set landing, because Hyprland's own rescue only
//! engages at zero binds and `binds.lua` always registers far more than zero -- so after the
//! reload, `hyprctl binds -j` is read back, and an empty result throws the overrides away
//! (back to the tracked default set), reloads again, and only then reports the failure. A set
//! that came out wrong without this check would simply be a desktop where a key does nothing,
//! discovered whenever the user next reached for it.

use garage_core::fs::atomic::atomic_write;
use garage_core::traits::DEFAULT_RUN_TIMEOUT;
use garage_prefs::PrefLock;
use garage_render::keybinds::{render_keybinds, Document};

use crate::cx::SessionCx;
use crate::keybind::catalog::{read_keybind_catalog, CatalogEntry};
use crate::keybind::guard::{guard_keybinds, known};
use crate::keybind::parse::{combination_id, require_bindable, CUSTOM_KEYBIND_MAX};
use crate::keybind::store::{custom_keybind, keybindings_toml, load_keybindings};
use crate::keybind::{KeybindError, KeybindRequest};

/// What `reload_keybinds()` writes when the reload left the desktop with nothing bound:
/// `json.dumps({"overrides": {}, "custom": []})`, whose default separators are `", "` and
/// `": "` -- deliberately *not* the `indent=2` layout
/// [`render_keybinds`](garage_render::keybinds::render_keybinds) writes, because this is the
/// Python's second, separate `json.dumps` call and the bytes are what they are.
const DEFAULT_KEYBINDS_JSON: &str = "{\"overrides\": {}, \"custom\": []}\n";

/// Apply one change to the shortcut set.
///
/// # Errors
///
/// [`KeybindError::Lock`] if the preferences lock cannot be taken, whatever the operation
/// itself refuses, whatever [`guard_keybinds`] refuses about the resulting set, and
/// [`KeybindError::Reload`] or [`KeybindError::NoBindsLeft`] if installing it left the
/// desktop worse off.
pub fn keybind_action(
    cx: &SessionCx<'_>,
    operation: &str,
    request: &KeybindRequest<'_>,
) -> Result<(), KeybindError> {
    let paths = cx.render().paths();
    // The lock is held from here to the end of this function: the load, the change, the
    // guard, the write, the render and the reload are one read-modify-write, and a stale
    // apply landing last would leave the desktop disagreeing with the file it came from.
    let lock = PrefLock::acquire(paths)?;
    let mut document = load_keybindings(paths, None);
    let (catalog, verified) = read_keybind_catalog(paths);
    apply_operation(&mut document, &catalog, operation, request)?;
    guard_keybinds(&catalog, &document, verified)?;
    atomic_write(&paths.host.keybindings, &keybindings_toml(&document)?)?;
    render_keybinds(cx.render(), &document)?;
    let reloaded = reload_keybinds(cx);
    drop(lock);
    reloaded
}

/// The six operations, on the document already loaded under the lock.
fn apply_operation(
    document: &mut Document,
    catalog: &[CatalogEntry],
    operation: &str,
    request: &KeybindRequest<'_>,
) -> Result<(), KeybindError> {
    match operation {
        "rebind" => rebind(document, catalog, request),
        "reset" => {
            document
                .overrides
                .remove(&combination_id(request.id.unwrap_or_default()));
            Ok(())
        }
        "reset-all" => {
            document.overrides = garage_render::keybinds::Overrides::new();
            Ok(())
        }
        "add" => {
            if document.custom.len() >= CUSTOM_KEYBIND_MAX {
                return Err(KeybindError::CustomFull(CUSTOM_KEYBIND_MAX));
            }
            document.custom.push(custom_keybind(request, "")?);
            Ok(())
        }
        "update" => update(document, request),
        "remove" => {
            let identifier = request.id.unwrap_or_default();
            document.custom.retain(|item| item.id != identifier);
            Ok(())
        }
        other => Err(KeybindError::UnknownOperation(other.to_owned())),
    }
}

/// Move one published bind to a combination of the user's choosing.
fn rebind(
    document: &mut Document,
    catalog: &[CatalogEntry],
    request: &KeybindRequest<'_>,
) -> Result<(), KeybindError> {
    let identifier = combination_id(request.id.unwrap_or_default());
    if known(catalog, &identifier).is_none() {
        return Err(KeybindError::NoSuchBind);
    }
    let keys = require_bindable(request.keys)?;
    // Back where it started is a removal, not an override that happens to agree: keeping it
    // would leave the pane showing the shortcut as changed, and pin it if the default ever
    // moved.
    if combination_id(&keys) == identifier {
        document.overrides.remove(&identifier);
    } else {
        document.overrides.set(&identifier, &keys);
    }
    Ok(())
}

/// Replace one custom shortcut, keeping its id.
fn update(document: &mut Document, request: &KeybindRequest<'_>) -> Result<(), KeybindError> {
    let identifier = request.id.unwrap_or_default();
    let Some(index) = document
        .custom
        .iter()
        .position(|item| item.id == identifier)
    else {
        return Err(KeybindError::NoSuchCustom);
    };
    let replacement = custom_keybind(request, identifier)?;
    if let Some(slot) = document.custom.get_mut(index) {
        *slot = replacement;
    }
    Ok(())
}

/// Reload, then prove the desktop still has keys.
///
/// # Errors
///
/// [`KeybindError::Reload`] if `hyprctl reload` refused, and [`KeybindError::NoBindsLeft`] if
/// it left the compositor with nothing bound -- after the defaults have been put back and
/// reloaded, which is why that is reported last rather than first.
pub fn reload_keybinds(cx: &SessionCx<'_>) -> Result<(), KeybindError> {
    let reloaded = cx.proc().run(&["hyprctl", "reload"], DEFAULT_RUN_TIMEOUT);
    match reloaded {
        // The Python's `run()` swallows a missing binary or a timeout into
        // `CompletedProcess(command, 1, "", str(error))`, so both arrive at the same refusal
        // as a non-zero exit: whatever the failure had to say, or the fallback sentence.
        Err(error) => return Err(KeybindError::Reload(refusal(&error.detail))),
        Ok(output) if output.status != 0 => {
            return Err(KeybindError::Reload(refusal(&output.stderr)))
        }
        Ok(_) => {}
    }
    if !desktop_has_no_binds(cx) {
        return Ok(());
    }
    atomic_write(
        &cx.render().paths().fragments.keybinds_data,
        DEFAULT_KEYBINDS_JSON,
    )?;
    drop(cx.proc().run(&["hyprctl", "reload"], DEFAULT_RUN_TIMEOUT));
    Err(KeybindError::NoBindsLeft)
}

/// `result.stderr.strip() or "Unable to reload the shortcuts"`.
fn refusal(stderr: &str) -> String {
    let said = stderr.trim();
    if said.is_empty() {
        "Unable to reload the shortcuts".to_owned()
    } else {
        said.to_owned()
    }
}

/// Whether `hyprctl binds -j` came back as an empty JSON list.
///
/// The Python decodes the whole document and asks `isinstance(live, list) and not live`. An
/// empty JSON array is `[`, whitespace, `]` and nothing else, so the two agree on every input
/// without a decoder here: any array with a member in it fails the whitespace test, anything
/// that is not an array fails the brackets, and text that does not parse at all is
/// `json_command`'s fallback -- which is `None`, and is not a list either.
fn desktop_has_no_binds(cx: &SessionCx<'_>) -> bool {
    let Ok(output) = cx
        .proc()
        .run(&["hyprctl", "binds", "-j"], DEFAULT_RUN_TIMEOUT)
    else {
        return false;
    };
    if output.status != 0 {
        return false;
    }
    let said = output.stdout.trim();
    said.strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .is_some_and(|inner| inner.trim().is_empty())
}
