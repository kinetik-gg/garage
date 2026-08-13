//! `guard_keybinds()`: refuse a shortcut set that could not be undone from the desktop
//! itself.
//!
//! Checked before installing, not after. Hyprland's own emergency binds only appear once the
//! bind count reaches zero, which cannot happen while `binds.lua` registers a hundred of
//! them, so a set that merely lost its terminal shortcut would go entirely uncaught by that
//! mechanism. The rescue binds are already structurally safe on the Lua side -- `bind()`
//! never consults an override for them -- and this is the second lock on the same door,
//! positioned so the pane can explain why a change is refused rather than the user finding
//! out by pressing the key and getting nothing.
//!
//! Three refusals, in order: the catalog must be non-empty (otherwise nothing has been
//! published yet), at least one rescue shortcut must exist in it (otherwise there is no way
//! back to a terminal even in principle), and every combination in the resolved set must be
//! unique. Then the rescue-resolution check: every rescue bind's combination must still
//! resolve to that rescue bind and no other.
//!
//! `verified` changes what an unknown id is *called*, never whether it is refused. Against a
//! catalog that cannot be proved whole -- see [`crate::keybind::catalog`] --
//! `load_keybindings()` no longer filters, so an override the fragment simply has not reached
//! arrives here and stops the change. Saying "there is no shortcut called super+s" about a
//! shortcut the user can see in the pane sends them to edit a file that is already correct;
//! the truth is that the list is mid-publication.

use std::collections::HashMap;

use garage_render::keybinds::Document;

use crate::keybind::catalog::{resolve_keybinds, CatalogEntry};
use crate::keybind::parse::combination_signature;
use crate::keybind::KeybindError;

/// Refuse a shortcut set that could not be undone from the desktop itself.
///
/// # Errors
///
/// [`KeybindError::NoCatalog`], [`KeybindError::NoRescue`], [`KeybindError::Unpublished`],
/// [`KeybindError::UnknownBind`], [`KeybindError::Protected`], [`KeybindError::Collision`] or
/// [`KeybindError::RescueMoved`], in that order of checking -- plus whatever
/// [`combination_signature`] refuses about a combination the catalog itself carries.
pub fn guard_keybinds(
    catalog: &[CatalogEntry],
    document: &Document,
    verified: bool,
) -> Result<(), KeybindError> {
    if catalog.is_empty() {
        return Err(KeybindError::NoCatalog);
    }
    let rescue: Vec<&CatalogEntry> = catalog.iter().filter(|entry| entry.protected).collect();
    if rescue.is_empty() {
        return Err(KeybindError::NoRescue);
    }
    for (identifier, _) in document.overrides.iter() {
        let Some(entry) = known(catalog, identifier) else {
            return Err(if verified {
                KeybindError::UnknownBind(identifier.to_owned())
            } else {
                KeybindError::Unpublished
            });
        };
        if entry.protected {
            return Err(KeybindError::Protected(entry.description.clone()));
        }
    }
    let claimed = claims(catalog, document)?;
    for entry in rescue {
        let signature = combination_signature(&entry.keys)?;
        if claimed.get(&signature) != Some(&entry.description) {
            return Err(KeybindError::RescueMoved {
                keys: entry.keys.clone(),
                description: entry.description.clone(),
            });
        }
    }
    Ok(())
}

/// The catalog entry an id names, which is the Python's `known = {entry["id"]: entry ...}`.
///
/// Searched from the end, because building that dict lets a later row overwrite an earlier
/// one with the same id. `binds.lua` cannot publish a duplicate id -- the id *is* the
/// combination, and two binds on one combination is what [`guard_keybinds`] refuses -- so
/// this only decides which of two identical ids a hand-written catalog is read as, and it
/// decides it the way the Python does.
pub(crate) fn known<'a>(catalog: &'a [CatalogEntry], id: &str) -> Option<&'a CatalogEntry> {
    catalog.iter().rev().find(|entry| entry.id == id)
}

/// Which description holds each combination, refusing the second claim on one.
///
/// Every match fires: `addKeybind` appends and the dispatch path runs all of them, so two
/// binds on one combination is not "the later one wins", it is both actions on every press.
fn claims(
    catalog: &[CatalogEntry],
    document: &Document,
) -> Result<HashMap<(Vec<String>, String), String>, KeybindError> {
    let mut claimed: HashMap<(Vec<String>, String), String> = HashMap::new();
    for entry in resolve_keybinds(catalog, document) {
        let signature = combination_signature(&entry.keys)?;
        if let Some(holder) = claimed.get(&signature) {
            return Err(KeybindError::Collision {
                keys: entry.keys.clone(),
                holder: holder.clone(),
            });
        }
        claimed.insert(signature, entry.description);
    }
    Ok(claimed)
}
