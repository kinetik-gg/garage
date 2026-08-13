//! Reading the three layers into one effective configuration.
//!
//! `load_toml()`, `shipped_defaults()` and `load_preferences()`, which is the whole read
//! path: layer 1 first, then layer 2 off the disk, then the migrations, then the coercion
//! pass that puts every key at a value this build can render.
//!
//! **`deep_merge()` has no counterpart here.** The Python merges the stored table over the
//! shipped defaults and hands the result to `validate_preferences()`;
//! [`Preferences::coerce_from`] takes the two separately and does the merge by construction,
//! since a key the stored table does not carry simply takes the shipped value. What the
//! merge additionally produced -- a *dict* still carrying whatever unknown keys the file
//! had -- has no representation in a typed configuration, and that is the one observable
//! difference: see [`crate::save::save_preferences`].

use std::fs;
use std::path::Path;

use garage_core::paths::Paths;
use garage_core::schema::notes::Notes;
use garage_core::schema::{Defaults, Preferences};

use crate::doc::report_preference_notes;
use crate::error::PrefsError;
use crate::migrate::migrate_preferences;

/// One TOML file as a table, or an empty one when it is not there.
///
/// The absent case is not an error because every file this reads is optional: a fresh
/// install has no `preferences.toml`, and the loader's answer for one is "the shipped
/// defaults, unmodified". A file that *is* there and cannot be read is a different thing
/// entirely, and it is reported rather than swallowed -- the values in it are the user's,
/// and quietly falling back to the defaults would look exactly like every setting having
/// been reset.
///
/// # Errors
///
/// [`PrefsError::Unreadable`], carrying the file's own name, when the file exists but cannot
/// be opened or does not parse.
pub fn load_toml(path: &Path) -> Result<toml::Table, PrefsError> {
    if !path.exists() {
        return Ok(toml::Table::new());
    }
    let text = fs::read_to_string(path).map_err(|error| PrefsError::unreadable(path, error))?;
    text.parse()
        .map_err(|error: toml::de::Error| PrefsError::unreadable(path, error))
}

/// Layer 1 as this process sees it: the shipped file, or the compiled-in copy.
///
/// A function rather than two reads because the load and the save have to agree on it
/// exactly. It is the base a load merges the stored file on top of, so it is also the base a
/// save subtracts to get the stored file back down to departures. Subtract a different base
/// -- the compiled copy while the load merged the TOML, say -- and any key where the two
/// disagree comes out of the save as a delta the load never saw, which is the fossil this is
/// here to prevent.
///
/// Missing and unreadable are deliberately different, and this is the Python's split rather
/// than a new one: `load_toml(DEFAULTS_PATH, FALLBACK_DEFAULTS)` falls back **silently** when
/// the file is absent -- no note, nothing on stderr -- and raises when it is present and
/// broken. Absent is the ordinary state of a checkout that has not been stowed yet, where
/// the compiled copy is the same file; broken is somebody having edited layer 1 by hand.
///
/// # Errors
///
/// [`PrefsError::Unreadable`] if the shipped file does not parse, and
/// [`PrefsError::Defaults`] if it parses but does not carry every key the schema declares,
/// or if the compiled copy itself cannot be read.
pub fn shipped_defaults(paths: &Paths) -> Result<Defaults, PrefsError> {
    if !paths.defaults_path.exists() {
        return Ok(Defaults::compiled()?);
    }
    let table = load_toml(&paths.defaults_path)?;
    Ok(Defaults::parse(&table).map_err(garage_core::schema::defaults::DefaultsError::from)?)
}

/// The effective configuration: layers 1 and 2, migrated, merged and coerced.
///
/// The order is the Python's and it matters twice over. Layer 1 is read *before* layer 2, so
/// a broken shipped file is what a session complains about rather than whatever the user's
/// own file happens to say. The migrations run *before* the merge, because the value
/// rewrites have to see the raw stored table -- merging the defaults in first would supply
/// the very version stamp they read to decide whether to fire.
///
/// `sink` is where notes go: `None` sends them to stderr, which is the journal under the
/// units that render at session start, and `Some` collects them for `garage doctor` to
/// count. Both the migration's dropped keys and the coercion pass's substitutions arrive
/// there, in that order.
///
/// # Errors
///
/// [`PrefsError`] if layer 1 cannot be read, if `preferences.toml` exists and does not
/// parse, or if the v5 rewrite reaches a value the emitter refuses. A value the *schema*
/// refuses is not an error: it is coerced and noted, because a single bad value used to take
/// the whole product down -- every render failed, and the one screen that could have
/// corrected it would not open either.
pub fn load_preferences(
    paths: &Paths,
    sink: Option<&mut Vec<String>>,
) -> Result<Preferences, PrefsError> {
    Ok(load_effective(paths, sink)?.preferences)
}

/// Everything `deep_merge(defaults, stored)` answers with, including the one section a
/// [`Preferences`] cannot hold.
///
/// The Python's `load_preferences()` returns a plain dict, and `[schema]` is in it: the
/// shipped defaults file carries the stamp, the merge keeps it, and every caller that prints
/// the configuration -- `set`'s envelope and `make_snapshot()`'s `preferences` field --
/// prints it too. A [`Preferences`] is the *validated* sections and deliberately has no room
/// for bookkeeping, so the stamp travels beside it rather than inside it, and only the two
/// printing callers ask for it.
#[derive(Debug, Clone)]
pub struct Effective {
    /// Layers 1 and 2, migrated, merged and coerced.
    pub preferences: Preferences,
    /// `deep_merge(defaults, stored)["schema"]`: the stamp section as the merged document
    /// carries it, or empty when neither layer had one.
    pub schema: toml::Table,
}

/// [`load_preferences`], keeping the `[schema]` stamp the merge produced.
///
/// # Errors
///
/// The same set [`load_preferences`] raises -- it is this function.
pub fn load_effective(
    paths: &Paths,
    mut sink: Option<&mut Vec<String>>,
) -> Result<Effective, PrefsError> {
    let defaults = shipped_defaults(paths)?;
    // Layer 1's own stamp, read from the file rather than from `Defaults`: the typed layer
    // has no room for it either, and the Python's `shipped_defaults()` is exactly this read.
    // A machine with no shipped file falls back to the compiled copy, which is
    // `FALLBACK_DEFAULTS` there -- derived from the schema table, and carrying no stamp.
    let mut schema = stamp_section(&if paths.defaults_path.exists() {
        load_toml(&paths.defaults_path)?
    } else {
        toml::Table::new()
    });
    let stored = load_toml(&paths.host.preferences)?;
    let stored = migrate_preferences(paths, stored, &defaults, sink.as_deref_mut())?;
    // `deep_merge` over the one section: layer 2's stamp wins key by key, and a key only
    // layer 1 has survives.
    schema.extend(stamp_section(&stored));
    let mut notes = Notes::new();
    let preferences = Preferences::coerce_from(&stored, &defaults, &mut notes);
    report_preference_notes(notes.as_slice(), sink);
    Ok(Effective {
        preferences,
        schema,
    })
}

/// One document's `[schema]` table, or an empty one when it has none or it is not a table.
fn stamp_section(document: &toml::Table) -> toml::Table {
    document
        .get("schema")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{load_toml, shipped_defaults};
    use crate::error::PrefsError;
    use crate::testing::World;

    #[test]
    fn an_absent_file_is_an_empty_table() {
        let world = World::new("absent");
        assert_eq!(
            load_toml(&world.paths().host.preferences).unwrap(),
            toml::Table::new()
        );
    }

    #[test]
    fn an_unparseable_file_is_named_by_its_own_filename() {
        let world = World::new("unparseable");
        world.plant_preferences("this is not toml\n");
        let error = load_toml(&world.paths().host.preferences).unwrap_err();
        assert!(
            matches!(&error, PrefsError::Unreadable { file, .. } if file == "preferences.toml"),
            "{error}"
        );
    }

    /// The Python's silent fallback: no defaults file at all is the ordinary state of an
    /// unstowed checkout, and the compiled copy is the same file.
    #[test]
    fn a_missing_defaults_file_falls_back_without_a_word() {
        let world = World::new("no-defaults");
        let defaults = shipped_defaults(world.paths()).unwrap();
        assert_eq!(
            defaults.values().appearance.accent_color.to_string(),
            "blue"
        );
    }

    #[test]
    fn a_shipped_file_that_does_not_parse_is_refused() {
        let world = World::new("broken-defaults");
        world.plant_defaults("nonsense\n");
        let error = shipped_defaults(world.paths()).unwrap_err();
        assert!(
            matches!(&error, PrefsError::Unreadable { file, .. }
                     if file == "preferences.defaults.toml"),
            "{error}"
        );
    }

    /// **Parity gap:** the Python coerces the absent key onto `FALLBACK_DEFAULTS` with a
    /// note and carries on. Layer 1 is typed here and cannot be built with a key missing.
    #[test]
    fn a_shipped_file_missing_a_key_is_refused_rather_than_coerced() {
        let world = World::new("incomplete-defaults");
        world.plant_defaults("[appearance]\naccent_color = \"blue\"\n");
        let error = shipped_defaults(world.paths()).unwrap_err();
        assert!(matches!(error, PrefsError::Defaults(_)), "{error}");
    }
}
