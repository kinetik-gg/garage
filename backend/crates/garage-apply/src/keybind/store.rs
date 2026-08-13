//! `load_keybindings()`, `keybindings_toml()` and `custom_keybind()`: reading and writing the
//! user's shortcut document.
//!
//! [`load_keybindings`] is lenient in the way `parse_workspace_counts()` is: this file is
//! small and obvious enough that someone will edit it by hand, and one bad line must not make
//! the whole document unloadable when the empty document is already the tracked default set --
//! the worst case is one shortcut quietly returning to what `binds.lua` gives it.
//!
//! Overrides are filtered against [`crate::keybind::catalog`]'s published set only when that
//! catalog is verified whole -- see its fail-closed witness-line contract -- because this
//! document is what a rebind renders back over `keybindings.toml`, and dropping an override
//! the reader could not vouch for would be a silent, permanent deletion of the user's choice
//! rather than a refusal the user could retry. An override naming a bind the verified catalog
//! genuinely no longer publishes is dropped and reported, the one case where the user does
//! lose something they chose from the pane and so the one worth a line in the journal.
//!
//! [`custom_keybind`] checks and normalises one user-defined shortcut. The command is never
//! inspected beyond its length and character set -- it is a shell command line, which is the
//! whole feature, and it reaches the compositor as data with nothing here for it to break out
//! of.

use std::fmt::Write as _;
use std::fs::File;
use std::io::Read as _;
use std::time::{SystemTime, UNIX_EPOCH};

use garage_core::paths::Paths;
use garage_core::toml_emit::{toml_value, Value};
use garage_prefs::load_toml;
use garage_render::keybinds::{CustomKeybind, Document, Overrides};

use crate::keybind::catalog::read_keybind_catalog;
use crate::keybind::parse::{
    combination_id, has_control_character, keybind_limit, require_bindable, CUSTOM_KEYBIND_MAX,
};
use crate::keybind::{KeybindError, KeybindRequest};

/// The user's shortcut changes, with anything unusable dropped.
///
/// `sink` is where the one note this can produce goes: `None` sends it to stderr, which is
/// the journal under the units that render at session start, and `Some` collects it. The
/// wording is the Python's, minus the `garage: keybindings.toml: ` prefix the stderr branch
/// adds -- the same split [`report_preference_notes`](garage_prefs::report_preference_notes)
/// draws.
pub fn load_keybindings(paths: &Paths, sink: Option<&mut Vec<String>>) -> Document {
    // `except SettingsError: raw = {}`: an unreadable or malformed file is the empty
    // document, which is the tracked default set.
    let raw = load_toml(&paths.host.keybindings).unwrap_or_default();
    let mut overrides = Overrides::new();
    if let Some(stored) = raw.get("overrides").and_then(toml::Value::as_table) {
        for (identifier, keys) in stored {
            if let Ok(bindable) = require_bindable(keys.as_str()) {
                overrides.set(&combination_id(identifier), &bindable);
            }
        }
    }
    filter_overrides(paths, &mut overrides, sink);
    let mut custom: Vec<CustomKeybind> = Vec::new();
    for item in raw
        .get("custom")
        .and_then(toml::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let Some(table) = item.as_table() else {
            continue;
        };
        let identifier = table.get("id").and_then(toml::Value::as_str).unwrap_or("");
        if let Ok(entry) = custom_keybind(&request_from(table), identifier) {
            custom.push(entry);
        }
    }
    custom.truncate(CUSTOM_KEYBIND_MAX);
    Document { overrides, custom }
}

/// One `[[custom]]` table as the payload `custom_keybind()` reads.
fn request_from(table: &toml::Table) -> KeybindRequest<'_> {
    KeybindRequest {
        id: table.get("id").and_then(toml::Value::as_str),
        keys: table.get("keys").and_then(toml::Value::as_str),
        description: table.get("description").and_then(toml::Value::as_str),
        command: table.get("command").and_then(toml::Value::as_str),
    }
}

/// Drop the overrides the *verified* catalog says are not shortcuts any more.
///
/// An override naming a shortcut that does not exist, or one `binds.lua` keeps for itself, is
/// dropped here rather than refused later. Left in, it would fail the guard on every
/// subsequent change and leave the pane unable to correct the very file that was blocking it.
///
/// Only against a catalog that proves itself whole, because this document is what
/// `keybind_action()` renders back over `keybindings.toml`: a key missing here is a rebind
/// deleted from the file. A truncated catalog reads as a complete short one, so filtering
/// against an unproven catalog turned a Hyprland reload racing a rebind into permanent,
/// silent data loss. Anything the reader cannot vouch for -- missing, empty, torn, or
/// published by a `binds.lua` older than the witness -- is treated the way a missing catalog
/// already was: read it, list it, change it, but never conclude from it that one of the
/// user's shortcuts has stopped existing.
///
/// What that costs is a refusal instead of a deletion. With an unverified catalog the
/// overrides reach `guard_keybinds()` unfiltered, so one naming a bind the fragment does not
/// carry makes the *current* change fail -- with "the shortcut list is still being
/// published", which is what it is -- until the catalog is republished. A change that did not
/// happen is recoverable; the rebinds it would have eaten were not.
fn filter_overrides(paths: &Paths, overrides: &mut Overrides, sink: Option<&mut Vec<String>>) {
    let (catalog, verified) = read_keybind_catalog(paths);
    if catalog.is_empty() || !verified {
        return;
    }
    // Reported, unlike the protected ids the filter also drops: an override on a rescue bind
    // is a hand edit the guard would refuse anyway, while an id the published set does not
    // carry at all is a shortcut this release removed or renamed -- the one case where the
    // user loses something they chose from the pane, and so the one worth a line in the
    // journal.
    let mut absent: Vec<String> = overrides
        .iter()
        .map(|(id, _)| id.to_owned())
        .filter(|id| !catalog.iter().any(|entry| entry.id == *id))
        .collect();
    absent.sort();
    if !absent.is_empty() {
        report(
            format!(
                "dropping {} override(s) on shortcut(s) config/binds.lua no longer publishes: {}",
                absent.len(),
                absent.join(", ")
            ),
            sink,
        );
    }
    overrides.retain(|id| {
        catalog
            .iter()
            .any(|entry| entry.id == id && !entry.protected)
    });
}

/// The Python's `print(..., file=sys.stderr)`, or the caller's collector.
fn report(note: String, sink: Option<&mut Vec<String>>) {
    match sink {
        None => eprintln!("garage: keybindings.toml: {note}"),
        Some(sink) => sink.push(note),
    }
}

/// One user-defined shortcut, checked and normalised.
///
/// # Errors
///
/// [`KeybindError::NoCommand`] for a shortcut with nothing to run, [`KeybindError::TooLong`]
/// or [`KeybindError::ControlCharacter`] for a field past its ceiling or carrying a control
/// character, and whatever [`require_bindable`] refuses about the combination.
pub fn custom_keybind(
    payload: &KeybindRequest<'_>,
    identifier: &str,
) -> Result<CustomKeybind, KeybindError> {
    let keys = require_bindable(payload.keys)?;
    let command = payload.command.unwrap_or_default().trim().to_owned();
    let description = payload.description.unwrap_or_default().trim().to_owned();
    if command.is_empty() {
        return Err(KeybindError::NoCommand);
    }
    for (field, text) in [
        ("keys", &keys),
        ("description", &description),
        ("command", &command),
    ] {
        let limit = keybind_limit(field);
        if text.chars().count() > limit {
            return Err(KeybindError::TooLong { field, limit });
        }
        if has_control_character(text) {
            return Err(KeybindError::ControlCharacter(field));
        }
    }
    Ok(CustomKeybind {
        id: if identifier.is_empty() {
            new_identifier()
        } else {
            identifier.to_owned()
        },
        keys,
        description,
        command,
    })
}

/// `uuid.uuid4().hex[:12]`: twelve hex characters nothing else is using.
///
/// Six bytes from `/dev/urandom`, which is what a v4 UUID's randomness comes from anyway.
/// The clock is the fallback for a machine where that cannot be opened -- a `chroot` with no
/// `/dev`, say -- and it is only ever an id in a file the same process is about to write, so
/// a collision costs the user one shortcut they can edit again rather than anything durable.
///
/// **Deviation, stated plainly:** the Python's ids are v4 UUID prefixes and these are not.
/// Nothing reads an id for its shape -- it is compared for equality and nothing else -- and
/// taking a UUID dependency for twelve hex characters would be the larger change.
fn new_identifier() -> String {
    let mut bytes = [0_u8; 6];
    let read = File::open("/dev/urandom").and_then(|mut source| source.read_exact(&mut bytes));
    if read.is_err() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        for (slot, byte) in bytes.iter_mut().zip(now.to_le_bytes()) {
            *slot = byte;
        }
    }
    bytes.iter().fold(String::new(), |mut out, byte| {
        // Writing into a `String` cannot fail; the result has nowhere to go.
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The document as `keybindings.toml`, byte for byte.
///
/// # Errors
///
/// [`KeybindError::Emit`] if a value could not be spelled, which this document's values --
/// all strings -- cannot produce.
pub fn keybindings_toml(document: &Document) -> Result<String, KeybindError> {
    let mut lines = vec![
        "# Keyboard shortcuts changed in System Preferences. Every shortcut not".to_owned(),
        "# named here keeps the combination config/binds.lua gives it.".to_owned(),
        String::new(),
        "[overrides]".to_owned(),
    ];
    // Quoted keys: an id is a combination with the spacing taken out, so it carries a "+" and
    // is never a bare TOML key.
    let mut moved: Vec<(&str, &str)> = document.overrides.iter().collect();
    moved.sort_unstable();
    for (identifier, keys) in moved {
        lines.push(format!(
            "{} = {}",
            toml_value(&Value::Str(identifier.to_owned()))?,
            toml_value(&Value::Str(keys.to_owned()))?
        ));
    }
    for item in &document.custom {
        lines.push(String::new());
        lines.push("[[custom]]".to_owned());
        for (key, text) in [
            ("id", &item.id),
            ("keys", &item.keys),
            ("description", &item.description),
            ("command", &item.command),
        ] {
            lines.push(format!(
                "{key} = {}",
                toml_value(&Value::Str(text.clone()))?
            ));
        }
    }
    Ok(lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::{custom_keybind, keybindings_toml, new_identifier};
    use crate::keybind::{Document, KeybindError, KeybindRequest, Overrides};

    #[test]
    fn an_empty_document_still_carries_its_header_and_section() {
        assert_eq!(
            keybindings_toml(&Document::default()).unwrap(),
            "# Keyboard shortcuts changed in System Preferences. Every shortcut not\n\
             # named here keeps the combination config/binds.lua gives it.\n\
             \n[overrides]\n"
        );
    }

    #[test]
    fn overrides_are_written_sorted_and_quoted() {
        let mut overrides = Overrides::new();
        overrides.set("super+z", "SUPER + Z");
        overrides.set("super+a", "SUPER + A");
        let document = Document {
            overrides,
            custom: Vec::new(),
        };
        let written = keybindings_toml(&document).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(
            lines.get(4..6),
            Some(["\"super+a\" = \"SUPER + A\"", "\"super+z\" = \"SUPER + Z\""].as_slice())
        );
    }

    #[test]
    fn a_custom_shortcut_needs_a_command() {
        let payload = KeybindRequest {
            keys: Some("SUPER + K"),
            ..KeybindRequest::default()
        };
        assert!(matches!(
            custom_keybind(&payload, ""),
            Err(KeybindError::NoCommand)
        ));
    }

    #[test]
    fn an_invented_id_is_twelve_hex_characters() {
        let identifier = new_identifier();
        assert_eq!(identifier.chars().count(), 12);
        assert!(identifier.chars().all(|ch| ch.is_ascii_hexdigit()));
    }
}
