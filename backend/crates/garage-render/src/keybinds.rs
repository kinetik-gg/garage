//! `render_keybinds()`: publish the shortcut set for `config/binds.lua` to read.
//!
//! A data file, and that is the point rather than a detail. The command a custom shortcut
//! runs is text the user typed, and generating a Lua `bind("...")` line from it would make
//! the field a place to write Lua: a command ending `")) ;` closes the call and everything
//! after it becomes config. Here the command is instead a JSON string that `binds.lua` reads
//! with a reader that can only ever return strings, so there is no route from the field to
//! the chunk.
//!
//! Reached from [`crate::all::render_all`], which calls it with the already-loaded
//! keybindings document, and from `garage-apply`'s `keybind_action()`, which renders the
//! document it has just written to `keybindings.toml`. Loading that document -- parsing
//! `keybindings.toml`, filtering overrides against the published catalog -- is
//! `garage-apply`'s concern and not this crate's, which is why this renderer takes the
//! document rather than reaching for it.
//!
//! # Why the document type lives here
//!
//! [`Document`] is the value `render_keybinds` consumes and the value `garage-apply`'s
//! `keybind` module produces, so it has to be nameable from both. `garage-apply` depends on
//! `garage-render` and never the other way round, so this crate is the lower of the two and
//! the type lives at the bottom of that edge. Everything that *decides* what belongs in a
//! document -- parsing a combination, reading the published catalog, refusing a set with no
//! way back to a terminal -- stays on the apply side; this file holds the shape and the
//! bytes it is written as.

use std::fmt::Write as _;

use garage_core::fs::atomic::atomic_write;

use crate::cx::RenderCx;
use crate::error::RenderError;

/// The user's shortcut changes: the binds they moved, and the ones they invented.
///
/// `load_keybindings()`'s return value, and the only thing `keybindings.toml` holds. The
/// empty document is the tracked default set -- every shortcut where `config/binds.lua`
/// puts it -- which is why a file that cannot be read at all is not an error but simply
/// this.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    /// Which published bind is bound to which combination instead of its default.
    pub overrides: Overrides,
    /// The shortcuts that have no default because the user invented them.
    pub custom: Vec<CustomKeybind>,
}

/// One user-defined shortcut, checked and normalised.
///
/// The command is never inspected beyond its length and its characters. It is a shell
/// command line, which is the whole feature -- `hl.dsp.exec_cmd` hands it to `/bin/sh` --
/// and it reaches the compositor as data, so there is nothing here it could break out of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomKeybind {
    /// The identifier the pane edits and removes it by. Twelve hex characters when the
    /// backend invented it.
    pub id: String,
    /// The combination, canonically spelled.
    pub keys: String,
    /// What the pane shows. May be empty, in which case the command stands in for it.
    pub description: String,
    /// The shell command line.
    pub command: String,
}

/// The overrides map: bind id to the combination it was moved to.
///
/// Ordered, and insertion-ordered specifically, because the Python's is a `dict` and the
/// order it was built in is the order `keybinds.json` is written in -- the file `binds.lua`
/// reads, and the file two backends have to produce identically. `keybindings.toml` sorts
/// its own lines, so the two orders are deliberately different and only this one is
/// positional.
///
/// [`Overrides::set`] is `dict.__setitem__`: an id already present keeps the position it
/// had and takes the new value, and only a new id is appended. Anything else would reorder
/// `keybinds.json` on a rebind of an existing override.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    entries: Vec<(String, String)>,
}

impl Overrides {
    /// An empty map.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// The combination this id was moved to, if it was moved.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, keys)| keys.as_str())
    }

    /// Whether this id carries an override.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// Set one id's combination, in place if it is already here and appended if it is not.
    pub fn set(&mut self, id: &str, keys: &str) {
        if let Some(slot) = self.entries.iter_mut().find(|(key, _)| key == id) {
            keys.clone_into(&mut slot.1);
        } else {
            self.entries.push((id.to_owned(), keys.to_owned()));
        }
    }

    /// Drop one id, if it is here at all. `dict.pop(id, None)`.
    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|(key, _)| key != id);
    }

    /// Keep only the ids the predicate accepts, in the order they are already in.
    pub fn retain(&mut self, keep: impl FnMut(&str) -> bool) {
        let mut keep = keep;
        self.entries.retain(|(key, _)| keep(key));
    }

    /// Every id and combination, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(key, keys)| (key.as_str(), keys.as_str()))
    }

    /// How many binds have been moved.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing has been moved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `keybinds.json`, byte for byte as `json.dumps(..., ensure_ascii=False, indent=2) + "\n"`
/// writes it.
///
/// `indent=2` fixes every detail of the layout: two spaces per level, `": "` between a key
/// and its value, `",\n"` between items, and an empty container written as `{}` or `[]` with
/// nothing inside it. `ensure_ascii=False` means a non-ASCII character in a description or a
/// command stays itself rather than becoming `\uXXXX`, which is what keeps a command with an
/// accent in it readable in the file the user can inspect.
#[must_use]
pub fn keybinds_json(document: &Document) -> String {
    let mut out = String::from("{\n  \"overrides\": ");
    if document.overrides.is_empty() {
        out.push_str("{}");
    } else {
        out.push_str("{\n");
        for (index, (id, keys)) in document.overrides.iter().enumerate() {
            if index > 0 {
                out.push_str(",\n");
            }
            out.push_str("    ");
            json_string(id, &mut out);
            out.push_str(": ");
            json_string(keys, &mut out);
        }
        out.push_str("\n  }");
    }
    out.push_str(",\n  \"custom\": ");
    if document.custom.is_empty() {
        out.push_str("[]");
    } else {
        out.push_str("[\n");
        for (index, item) in document.custom.iter().enumerate() {
            if index > 0 {
                out.push_str(",\n");
            }
            custom_object(item, &mut out);
        }
        out.push_str("\n  ]");
    }
    out.push_str("\n}\n");
    out
}

/// One custom shortcut as `{key: item[key] for key in ("keys", "description", "command")}`:
/// three fields, in that order, and never the id -- `binds.lua` has no use for it, and the
/// helper that wrote it already knows it.
fn custom_object(item: &CustomKeybind, out: &mut String) {
    out.push_str("    {\n      \"keys\": ");
    json_string(&item.keys, out);
    out.push_str(",\n      \"description\": ");
    json_string(&item.description, out);
    out.push_str(",\n      \"command\": ");
    json_string(&item.command, out);
    out.push_str("\n    }");
}

/// `json.dumps(text, ensure_ascii=False)` for one string.
///
/// The same six named escapes `CPython`'s `json.encoder.encode_basestring` writes -- `\` `"`
/// `\b` `\f` `\n` `\r` `\t` -- with everything else below U+0020 as `\u00xx` in lowercase
/// hex, and nothing at all above it: U+007F and the C1 controls go through verbatim, and
/// `ensure_ascii=False` leaves every non-ASCII character alone.
///
/// A deliberate second copy of `garage_core::toml_emit`'s private `json_string`, which is
/// the same encoder behind `toml_value()`. Making it shared would mean widening that
/// module's API for one caller in another crate, and the rule -- "below U+0020, or one of
/// two ASCII characters" -- is short enough that a copy with this docstring beside it is
/// cheaper to keep honest than a dependency edge.
fn json_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            // Writing into a `String` cannot fail, so the result is dropped rather than
            // propagated into a signature with nowhere to put it.
            _ if ch < '\u{20}' => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
}

/// Write the resolved shortcut set for `config/binds.lua` to read.
///
/// The Python creates `generated/` first; [`atomic_write`] already creates the directory a
/// file belongs in, so the `mkdir` is not repeated here.
///
/// # Errors
///
/// [`RenderError::Atomic`] if `keybinds.json` could not be written.
pub fn render_keybinds(cx: &RenderCx<'_>, document: &Document) -> Result<(), RenderError> {
    atomic_write(
        &cx.paths().fragments.keybinds_data,
        &keybinds_json(document),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{keybinds_json, CustomKeybind, Document, Overrides};

    #[test]
    fn an_empty_document_writes_both_containers_empty() {
        assert_eq!(
            keybinds_json(&Document::default()),
            "{\n  \"overrides\": {},\n  \"custom\": []\n}\n"
        );
    }

    #[test]
    fn setting_an_existing_id_keeps_its_position() {
        let mut overrides = Overrides::new();
        overrides.set("super+a", "SUPER + A");
        overrides.set("super+b", "SUPER + B");
        overrides.set("super+a", "SUPER + C");
        let seen: Vec<(&str, &str)> = overrides.iter().collect();
        assert_eq!(
            seen,
            [("super+a", "SUPER + C"), ("super+b", "SUPER + B")],
            "a rebind of an existing override must not reorder keybinds.json"
        );
    }

    #[test]
    fn a_custom_shortcut_is_written_without_its_id() {
        let document = Document {
            overrides: Overrides::new(),
            custom: vec![CustomKeybind {
                id: "0123456789ab".to_owned(),
                keys: "SUPER + K".to_owned(),
                description: "Say\thello".to_owned(),
                command: "notify-send \"hi\"".to_owned(),
            }],
        };
        let written = keybinds_json(&document);
        assert!(!written.contains("0123456789ab"));
        assert!(written.contains("\"description\": \"Say\\thello\""));
        assert!(written.contains("\"command\": \"notify-send \\\"hi\\\"\""));
    }
}
