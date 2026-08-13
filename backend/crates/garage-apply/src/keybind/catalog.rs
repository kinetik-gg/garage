//! `read_keybind_catalog()`: the published bind set, and whether it can be proved whole.
//!
//! Read rather than derived. `hyprctl binds -j` reports where a bind ended up and an opaque
//! Lua registry index, never the combination the file wanted -- and the two differ in exactly
//! the case there is something worth showing. Parsing `binds.lua` from here would be a
//! second, worse Lua interpreter that would have to know what the workspace loop expands to.
//!
//! # The fail-closed witness-line contract
//!
//! Every line of the published catalog is self-describing, so a truncated file reads back as
//! a complete but shorter one, and nothing in the individual entries marks it as a fragment.
//! That is precisely how a reader that filtered the user's overrides against it once came to
//! delete overrides it simply had not seen yet, mid-publication. So `config/binds.lua` writes
//! a witness line last -- `#end<TAB>N`, with `N` the number of rows -- and
//! [`read_keybind_catalog`] reports the catalog as unverified unless the file both ends with
//! that line and `N` matches the number of rows actually parsed. Fail-closed: any tear, any
//! short count, any missing witness leaves the catalog unverified, and every caller
//! downstream may still read it and list it, but must never conclude from it that a shortcut
//! the catalog does not mention has ceased to exist.
//!
//! False does not mean broken. A session still running a copy of `binds.lua` that shipped
//! before the witness line existed publishes a perfectly good catalog with no last line, and
//! it must keep working for everything except that one conclusion.
//!
//! [`keybind_catalog`] is the plain reader built on top, for callers that only want the
//! default set and do not need the verified flag -- `keybindings_snapshot()` is one.
//! [`resolve_keybinds`] is the whole bind set `config/binds.lua` would register from a
//! document: each catalog entry keeps its default unless it is overridden and not protected,
//! plus every custom shortcut appended after it.

use std::fs;

use garage_core::paths::Paths;
use garage_render::keybinds::Document;

use crate::keybind::parse::KEYBIND_CATALOG_SENTINEL;

/// One row of the published catalog: `id \t group \t keys \t protected \t description`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// `bind_id(keys)`: the default combination, whitespace removed and lowercased.
    pub id: String,
    /// Which section of `binds.lua` the bind was declared in, so the pane can group them
    /// the way the file already does.
    pub group: String,
    /// The combination as the Lua file wrote it.
    pub keys: String,
    /// Whether the bind is in `binds.lua`'s `RESCUE` table, which no override may move.
    pub protected: bool,
    /// What the pane shows.
    pub description: String,
}

/// One resolved bind: a catalog entry with the user's document applied, or a custom shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBind {
    /// The catalog id, or the custom shortcut's own id.
    pub id: String,
    /// The catalog's group, or `Custom`.
    pub group: String,
    /// Where the bind actually is.
    pub keys: String,
    /// Where `binds.lua` puts it, so the pane can offer "restore". Empty for a custom
    /// shortcut, which has no default by definition.
    pub default_keys: String,
    /// Whether it is a rescue bind.
    pub protected: bool,
    /// What the pane shows. A custom shortcut with no description shows its command.
    pub description: String,
    /// Whether an override moved it.
    pub modified: bool,
    /// Whether the user invented it.
    pub custom: bool,
}

/// The published bind set, and whether it can be proved whole.
///
/// A file that cannot be read at all is `([], false)` -- the same answer as a torn one, and
/// deliberately so: neither can be acted on.
///
/// The Python splits with `str.splitlines()`, which also breaks on `\v`, `\f`, `\x1c`-`\x1e`,
/// `\x85` and the two Unicode separators; this splits on newlines alone. `binds.lua` writes
/// only fields it authored and promises none of them contains a tab or a newline, so the
/// difference is unreachable from the writer -- and a hand-mangled file carrying one of those
/// characters fails the witness count here rather than being read as an extra row.
#[must_use]
pub fn read_keybind_catalog(paths: &Paths) -> (Vec<CatalogEntry>, bool) {
    let Ok(text) = fs::read_to_string(&paths.fragments.keybinds_catalog) else {
        return (Vec::new(), false);
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut catalog = Vec::new();
    for line in &lines {
        let fields: Vec<&str> = line.split('\t').collect();
        let [id, group, keys, protected, description] = fields.as_slice() else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        catalog.push(CatalogEntry {
            id: (*id).to_owned(),
            group: (*group).to_owned(),
            keys: (*keys).to_owned(),
            protected: *protected == "1",
            description: (*description).to_owned(),
        });
    }
    let witness = lines
        .iter()
        .rev()
        .find(|line| !line.trim().is_empty())
        .copied()
        .unwrap_or_default();
    let verified = witness == format!("{KEYBIND_CATALOG_SENTINEL}\t{}", catalog.len());
    (catalog, verified)
}

/// The default bind set alone, for everything that only has to display it.
#[must_use]
pub fn keybind_catalog(paths: &Paths) -> Vec<CatalogEntry> {
    read_keybind_catalog(paths).0
}

/// The whole bind set `config/binds.lua` would register from this document.
#[must_use]
pub fn resolve_keybinds(catalog: &[CatalogEntry], document: &Document) -> Vec<ResolvedBind> {
    let mut resolved = Vec::with_capacity(catalog.len() + document.custom.len());
    for entry in catalog {
        let keys = if entry.protected {
            entry.keys.clone()
        } else {
            document
                .overrides
                .get(&entry.id)
                .unwrap_or(&entry.keys)
                .to_owned()
        };
        resolved.push(ResolvedBind {
            id: entry.id.clone(),
            group: entry.group.clone(),
            modified: keys != entry.keys,
            keys,
            default_keys: entry.keys.clone(),
            protected: entry.protected,
            description: entry.description.clone(),
            custom: false,
        });
    }
    for item in &document.custom {
        resolved.push(ResolvedBind {
            id: item.id.clone(),
            group: "Custom".to_owned(),
            keys: item.keys.clone(),
            default_keys: String::new(),
            protected: false,
            description: if item.description.is_empty() {
                item.command.clone()
            } else {
                item.description.clone()
            },
            modified: false,
            custom: true,
        });
    }
    resolved
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use garage_core::paths::Paths;
    use garage_render::keybinds::{Document, Overrides};

    use super::{read_keybind_catalog, resolve_keybinds};
    use crate::keybind::parity::scratch_paths;

    fn write_catalog(paths: &Paths, text: &str) {
        let target = &paths.fragments.keybinds_catalog;
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }

    #[test]
    fn a_missing_catalog_is_empty_and_unverified() {
        let env: HashMap<String, String> = [("HOME".to_owned(), "/nonexistent".to_owned())]
            .into_iter()
            .collect();
        let paths = Paths::from_env_map(&env);
        assert_eq!(read_keybind_catalog(&paths), (Vec::new(), false));
    }

    #[test]
    fn a_protected_entry_ignores_its_override() {
        let paths = scratch_paths("catalog-protected");
        write_catalog(
            &paths,
            "super+return\tApplications\tSUPER + Return\t1\tOpen a terminal\n#end\t1\n",
        );
        let (catalog, verified) = read_keybind_catalog(&paths);
        assert!(verified);
        let mut overrides = Overrides::new();
        overrides.set("super+return", "SUPER + T");
        let document = Document {
            overrides,
            custom: Vec::new(),
        };
        let resolved = resolve_keybinds(&catalog, &document);
        assert_eq!(
            resolved.first().map(|bind| bind.keys.as_str()),
            Some("SUPER + Return")
        );
        assert_eq!(resolved.first().map(|bind| bind.modified), Some(false));
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
