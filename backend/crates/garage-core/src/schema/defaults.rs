//! Layer 1: `preferences.defaults.toml`, the shipped source of truth for
//! values.
//!
//! The original implementation derived its fallback values from the table and
//! kept the file as a second, hand-editable copy. This port keeps the useful
//! split, with the arrow reversed: the table declares the keys and their kinds,
//! the *file* carries the values, and [`Defaults::compiled`] reads the file at
//! build time so a Rust build cannot ship a default the file does not have.
//!
//! That is why a missing key is [`MissingDefault`] rather than a coercion.
//! Every other failure in this module puts a value back and writes a note,
//! because the alternative is a desktop that will not render; a default that is
//! absent or of the wrong type has nothing to put back, so it has to be loud,
//! and the tests below are what make it loud at build time instead of at
//! session start.

use crate::schema::coerce::Coerce;
use crate::schema::prefs::{PreferenceKey, Preferences, Section};

/// `preferences.defaults.toml`, read at build time.
///
/// The path is relative to this file: five levels up is the repository root,
/// where `desktop/` is the stow tree the session actually runs from.
const COMPILED: &str =
    include_str!("../../../../../desktop/.config/garage/preferences.defaults.toml");

/// The shipped defaults could not answer for one key.
#[derive(Copy, Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error(
    "preferences.defaults.toml is missing or mistyped {key}; this build cannot render without it"
)]
pub struct MissingDefault {
    /// The key with no usable default.
    pub key: PreferenceKey,
}

/// Why the compiled defaults could not be read.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum DefaultsError {
    /// One key is absent or stored at a type the schema does not declare.
    #[error(transparent)]
    Missing(#[from] MissingDefault),
    /// The file itself does not parse. Only reachable by editing the shipped
    /// file into something TOML refuses; the tests below catch it first.
    #[error("preferences.defaults.toml does not parse: {0}")]
    Unparsable(String),
}

/// Layer 1, parsed and typed once.
///
/// Holds a whole [`Preferences`] rather than the raw table: every default has
/// already been read at the type its key declares, so the coercion pass can
/// hand one out without re-parsing and without a second way to fail.
#[derive(Clone, Debug, PartialEq)]
pub struct Defaults {
    values: Preferences,
}

impl Defaults {
    /// Read every default out of an already-parsed table.
    ///
    /// The caller provides the table: reading the file is the shell's job, not
    /// this crate's, and the one file this crate does read is compiled in.
    ///
    /// # Errors
    ///
    /// [`MissingDefault`] for the first key the table does not carry at the
    /// type the schema declares.
    pub fn parse(table: &toml::Table) -> Result<Self, MissingDefault> {
        Ok(Self {
            values: Preferences::from_defaults(table)?,
        })
    }

    /// The defaults this build ships, from the file itself.
    ///
    /// # Errors
    ///
    /// [`DefaultsError`] when the shipped file does not parse or does not
    /// carry every key -- both of which are broken-build conditions rather
    /// than anything a session can recover from.
    pub fn compiled() -> Result<Self, DefaultsError> {
        let table: toml::Table = COMPILED
            .parse()
            .map_err(|error: toml::de::Error| DefaultsError::Unparsable(error.to_string()))?;
        Ok(Self::parse(&table)?)
    }

    /// The defaults as a whole configuration -- what an empty
    /// `preferences.toml` resolves to.
    #[must_use]
    pub const fn values(&self) -> &Preferences {
        &self.values
    }

    /// The shipped value for one key, as it is stored.
    #[must_use]
    pub fn get(&self, key: PreferenceKey) -> toml::Value {
        self.values.get(key)
    }
}

/// One default out of a parsed table, at the type its key declares.
///
/// The lookup is `[section][key]`, because the defaults file is the same shape
/// as `preferences.toml` -- nested section, then key -- and that is the shape
/// every consumer already expects.
///
/// # Errors
///
/// [`MissingDefault`] when the section is absent, the key is absent, or the
/// value is not one the key's kind accepts.
pub fn require<T: Coerce>(table: &toml::Table, key: PreferenceKey) -> Result<T, MissingDefault> {
    table
        .get(key.section().as_str())
        .and_then(toml::Value::as_table)
        .and_then(|section| section.get(key.name()))
        .and_then(T::coerce)
        .ok_or(MissingDefault { key })
}

/// Keys the schema used to have, dropped from the merged view on every load.
///
/// A file that somehow carries both a version stamp and an old key must not
/// write it back out on every save. The wallpaper singles are pre-v3 -- see
/// `WALLPAPER_SPLIT` and `migrate_preference_values()`, which is where they are
/// carried across rather than lost -- and `highlight_color` was withdrawn
/// outright.
///
/// Dropped last in the Python, and for the one reason that makes the position
/// safe: no check reads a withdrawn key, and nothing after them does either.
pub const WITHDRAWN_KEYS: &[(Section, &str)] = &[
    (Section::Appearance, "wallpaper"),
    (Section::Appearance, "wallpaper_source"),
    (Section::Appearance, "wallpaper_color"),
    (Section::Appearance, "highlight_color"),
];

/// Whether a stored key is one this build withdrew.
#[must_use]
pub fn is_withdrawn(section: Section, key: &str) -> bool {
    WITHDRAWN_KEYS
        .iter()
        .any(|(withdrawn, name)| *withdrawn == section && *name == key)
}

/// Pop every withdrawn key out of a stored table, silently.
///
/// [`Preferences::coerce_from`] does not need this -- it reads the keys the
/// table declares and can no more see a withdrawn one than an invented one --
/// but the file the session writes back is built from the stored table, so the
/// keys have to leave *it* or they survive every save.
pub fn strip_withdrawn(stored: &mut toml::Table) {
    for (section, key) in WITHDRAWN_KEYS {
        if let Some(table) = stored
            .get_mut(section.as_str())
            .and_then(toml::Value::as_table_mut)
        {
            table.remove(*key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_withdrawn, strip_withdrawn, Defaults, COMPILED, WITHDRAWN_KEYS};
    use crate::schema::prefs::{PreferenceKey, Section};

    fn compiled_table() -> toml::Table {
        COMPILED.parse().unwrap()
    }

    /// The whole point of layer 1: the shipped file answers for every key in
    /// the table, at the type the table declares for it.
    ///
    /// Reading each one back out and comparing it to the file proves two
    /// things at once -- the key is there at a type its kind accepts, and the
    /// trip through that type did not quietly restate the value. A default
    /// this build would rewrite is a default that departs from itself.
    #[test]
    fn defaults_file_has_every_key() {
        let table = compiled_table();
        let defaults = Defaults::compiled().unwrap();
        for key in PreferenceKey::ALL.iter().copied() {
            let stored = table
                .get(key.section().as_str())
                .and_then(toml::Value::as_table)
                .and_then(|section| section.get(key.name()));
            assert_eq!(Some(&defaults.get(key)), stored, "{key}");
        }
    }

    #[test]
    fn removing_a_key_is_a_startup_error() {
        let mut table = compiled_table();
        table
            .get_mut("appearance")
            .and_then(toml::Value::as_table_mut)
            .unwrap()
            .remove("accent_color");
        let error = Defaults::parse(&table).unwrap_err();
        assert_eq!(error.key, PreferenceKey::AccentColor);
        assert_eq!(
            error.to_string(),
            "preferences.defaults.toml is missing or mistyped appearance.accent_color; \
             this build cannot render without it"
        );
    }

    #[test]
    fn mistyping_a_key_is_the_same_error() {
        let mut table = compiled_table();
        table
            .get_mut("bar")
            .and_then(toml::Value::as_table_mut)
            .unwrap()
            .insert(
                "height".to_string(),
                toml::Value::String("tall".to_string()),
            );
        assert_eq!(
            Defaults::parse(&table).unwrap_err().key,
            PreferenceKey::BarHeight
        );
    }

    #[test]
    fn a_default_outside_its_own_range_is_missing_too() {
        let mut table = compiled_table();
        table
            .get_mut("appearance")
            .and_then(toml::Value::as_table_mut)
            .unwrap()
            .insert("border_size".to_string(), toml::Value::Integer(99));
        assert_eq!(
            Defaults::parse(&table).unwrap_err().key,
            PreferenceKey::BorderSize
        );
    }

    #[test]
    fn withdrawn_keys_leave_the_stored_table() {
        let mut stored: toml::Table = "[appearance]\nwallpaper = \"old.png\"\n\
             highlight_color = \"#ff0000\"\naccent_color = \"teal\"\n"
            .parse()
            .unwrap();
        strip_withdrawn(&mut stored);
        let appearance = stored.get("appearance").unwrap().as_table().unwrap();
        assert!(!appearance.contains_key("wallpaper"));
        assert!(!appearance.contains_key("highlight_color"));
        assert_eq!(appearance.len(), 1);
        assert!(is_withdrawn(Section::Appearance, "wallpaper_source"));
        assert!(!is_withdrawn(Section::Appearance, "wallpaper_light"));
        assert!(!is_withdrawn(Section::Bar, "wallpaper"));
        assert_eq!(WITHDRAWN_KEYS.len(), 4);
    }
}
