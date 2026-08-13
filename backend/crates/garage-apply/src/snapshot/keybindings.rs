//! `keybindings_snapshot()`: the shortcut list the pane draws, grouped the way `binds.lua` is
//! written.
//!
//! Resolves the published catalog against the user's document -- see
//! [`crate::keybind::catalog`] for the fail-closed witness-line contract that governs whether
//! an override can be trusted against it -- and groups the result by the catalog's own group
//! names, in first-appearance order, so the pane's sections mirror `binds.lua`'s own
//! organisation rather than an alphabetical resort. Custom shortcuts are reported separately
//! from the grouped defaults, deep-copied so the pane's own mutation of the response cannot
//! reach back into the loaded document.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.

use garage_render::keybinds::{CustomKeybind, Document};
use serde_json::{json, Map, Value};

use crate::cx::SessionCx;
use crate::keybind::catalog::{keybind_catalog, resolve_keybinds};
use crate::keybind::parse::key_choices;

/// The four modifiers the pane offers, in the Python's own order (garage:5008).
const MODIFIERS: [&str; 4] = ["SUPER", "CONTROL", "ALT", "SHIFT"];

/// `keybindings_snapshot()` (garage:3462-3482): the shortcut list, grouped the way `binds.lua`
/// is written.
pub(crate) fn keybindings_snapshot(cx: &SessionCx<'_>, document: &Document) -> Value {
    let catalog = keybind_catalog(cx.render().paths());
    // First-appearance order, not an alphabetical resort: the pane's sections mirror
    // `binds.lua`'s own organisation, which is the whole reason the catalog carries a group.
    let mut titles: Vec<String> = Vec::new();
    let mut binds: Vec<Vec<Value>> = Vec::new();
    for entry in resolve_keybinds(&catalog, document) {
        if entry.custom {
            continue;
        }
        let existing = titles.iter().position(|title| *title == entry.group);
        let slot = if let Some(found) = existing {
            found
        } else {
            titles.push(entry.group.clone());
            binds.push(Vec::new());
            titles.len() - 1
        };
        if let Some(group) = binds.get_mut(slot) {
            group.push(json!({
                "id": entry.id,
                "group": entry.group,
                "keys": entry.keys,
                "defaultKeys": entry.default_keys,
                "protected": entry.protected,
                "description": entry.description,
                "modified": entry.modified,
                "custom": entry.custom,
            }));
        }
    }
    let groups: Vec<Value> = titles
        .into_iter()
        .zip(binds)
        .map(|(title, group)| json!({ "title": title, "binds": group }))
        .collect();
    json!({
        "available": !catalog.is_empty(),
        "groups": groups,
        // Deep-copied there so the pane's own mutation of the response cannot reach back into
        // the loaded document; here the copy is what building a fresh value already is.
        "custom": document.custom.iter().map(custom_value).collect::<Vec<Value>>(),
        "modifiers": MODIFIERS,
        "keys": key_choices(),
    })
}

/// One custom shortcut, in `custom_keybind()`'s own key order (garage:3168-3169).
fn custom_value(item: &CustomKeybind) -> Value {
    let mut record = Map::new();
    record.insert("id".to_owned(), Value::String(item.id.clone()));
    record.insert("keys".to_owned(), Value::String(item.keys.clone()));
    record.insert(
        "description".to_owned(),
        Value::String(item.description.clone()),
    );
    record.insert("command".to_owned(), Value::String(item.command.clone()));
    Value::Object(record)
}
