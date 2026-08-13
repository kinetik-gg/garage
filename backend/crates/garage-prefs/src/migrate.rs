//! The three migrations, and the stamp that drives all of them.
//!
//! One version number lives in `preferences.toml`, and three separate things read it. The
//! file move ([`migrate_config_root`](crate::config_root::migrate_config_root)) has to
//! happen before the file is read at all, so it lives in [`crate::config_root`]; the value
//! rewrites ([`migrate_preference_values`]) happen in memory on the raw stored table; the
//! shape rewrite ([`compact_preferences_file`]) rewrites the file itself, on the load path,
//! under the preferences lock. [`migrate_preferences`] is the orchestration of the last two,
//! and it is what a load calls.

use garage_core::fs::atomic::atomic_write;
use garage_core::paths::Paths;
use garage_core::schema::notes::py_str_toml;
use garage_core::schema::Defaults;

use crate::doc::{emit_document, preference_document, preference_sections};
use crate::error::PrefsError;
use crate::load::load_toml;
use crate::lock::PrefLock;
use crate::pyvalue::py_equal_table;

/// Stamp carried in `preferences.toml`, read by [`migrate_preferences`]. Bump it, and add
/// the coercion there, whenever a stored value changes meaning rather than just changing
/// range -- or, as v5 did, whenever the file's own shape changes and an existing one has to
/// be rewritten to match.
pub const PREFERENCES_VERSION: i64 = 5;

/// How the old three corner radii map onto the new three: every old corner keeps its px, so
/// a migrated desktop looks exactly as it did. Applied once, gated on the schema version,
/// because "normal" appears on both sides and would otherwise walk a setting from normal to
/// large on every load.
const CORNER_RADIUS_RENAMES: [(&str, &str); 3] =
    [("small", "normal"), ("normal", "large"), ("large", "large")];

/// What the single pre-v3 wallpaper key of each kind became. The old value goes into both
/// halves rather than being dropped, so a migrated desktop keeps the picture it had until
/// the two are deliberately set apart.
const WALLPAPER_SPLIT: [(&str, &str); 3] = [
    ("wallpaper", ""),
    ("wallpaper_source", "_source"),
    ("wallpaper_color", "_color"),
];

/// The two appearances, which are also the suffixes the per-appearance wallpaper keys are
/// spelled with: `wallpaper_light`, `wallpaper_dark_source`, and so on.
const SCHEMES: [&str; 2] = ["light", "dark"];

/// The stamp a stored `preferences.toml` carries, or 1 if it carries none.
///
/// A hand-mangled stamp is treated as older than current rather than fatal: refusing the
/// file here would leave the whole UI read-only. A bool is not an int for this purpose --
/// Python has to say so explicitly, since `True` *is* an `int` there; `as_integer()` answers
/// `None` for one here, which is the same judgement arrived at for free.
#[must_use]
pub fn schema_version(stored: &toml::Table) -> i64 {
    stored
        .get("schema")
        .and_then(toml::Value::as_table)
        .and_then(|schema| schema.get("preferences_version"))
        .and_then(toml::Value::as_integer)
        .unwrap_or(1)
}

/// The value steps of the migration, in memory, on the raw stored table.
///
/// Raw rather than merged on purpose. The corner radius rename reuses "normal" for what used
/// to be called "small", so the value alone cannot say which scale wrote it -- only the
/// absence of the version stamp can, and merging the defaults in first would supply one.
///
/// Each step is gated on the version that introduced it rather than on "older than current".
/// A single "not current yet" gate replays every step whenever the stamp is bumped, which is
/// precisely how the corner radius walked from normal to large the first time a later
/// version was added.
///
/// v4 is not here: it moves files rather than values, so it has to run before the read. See
/// [`migrate_config_root`](crate::config_root::migrate_config_root). v5 is not here either: it changes the shape of the whole file
/// rather than any value in it, and it rewrites the file to do so. See
/// [`compact_preferences_file`].
///
/// **Parity gap, stated plainly:** the Python's last line is
/// `stored.setdefault("schema", {})["preferences_version"] = PREFERENCES_VERSION`, which
/// raises `TypeError` on a file whose top-level `schema` is not a table (`schema = 5`) and
/// takes the process down with it -- `main()` does not catch `TypeError`. Here that file is
/// left without a stamp instead, which is the same outcome as every other mangled stamp: the
/// migration replays on the next load and the effective configuration is unchanged.
pub fn migrate_preference_values(stored: &mut toml::Table) {
    let version = schema_version(stored);
    if version >= PREFERENCES_VERSION {
        return;
    }
    if let Some(appearance) = stored
        .get_mut("appearance")
        .and_then(toml::Value::as_table_mut)
    {
        // v2 renamed the corner radius scale.
        if version < 2 {
            rename_corner_radius(appearance);
        }
        // v3 gave each appearance its own wallpaper.
        if version < 3 {
            split_wallpapers(appearance);
        }
    }
    if let Some(schema) = stored
        .entry("schema".to_owned())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
    {
        schema.insert(
            "preferences_version".to_owned(),
            toml::Value::Integer(PREFERENCES_VERSION),
        );
    }
}

/// v2's rename, applied to whatever the file happens to hold.
///
/// The lookup is on `str(value)`, so a stored value that is not a string cannot match a
/// spelling and is left exactly as it is -- for the coercion pass to refuse later.
fn rename_corner_radius(appearance: &mut toml::Table) {
    let Some(current) = appearance.get("corner_radius") else {
        return;
    };
    let spelling = py_str_toml(current);
    let renamed = CORNER_RADIUS_RENAMES
        .iter()
        .find(|(old, _)| *old == spelling);
    if let Some((_, new)) = renamed {
        appearance.insert(
            "corner_radius".to_owned(),
            toml::Value::String((*new).to_owned()),
        );
    }
}

/// v3's split. Insert-if-absent rather than assignment: a file written between the two
/// schemas could already carry a half, and a deliberate choice outranks the value it was
/// split from.
fn split_wallpapers(appearance: &mut toml::Table) {
    for (legacy, suffix) in WALLPAPER_SPLIT {
        let Some(value) = appearance.remove(legacy) else {
            continue;
        };
        for scheme in SCHEMES {
            let key = format!("wallpaper_{scheme}{suffix}");
            if !appearance.contains_key(&key) {
                appearance.insert(key, value.clone());
            }
        }
    }
}

/// v5: shrink a stored full document down to the departures it always meant.
///
/// Every version up to 4 wrote `preferences.toml` as the whole merged configuration, so an
/// install that ever changed one setting carries a frozen copy of all ~50 shipped defaults --
/// and that copy outranks layer 1 forever, which means no later change to any shipped
/// default could ever reach the machine again, and a key withdrawn from the schema could
/// never leave it. Rewriting the file the first time this build reads it is what
/// un-fossilizes them. The effective configuration is untouched by the rewrite: a stored
/// value that still differs from today's default is a departure and stays, and one that
/// equals it is dropped only because the merge puts it straight back.
///
/// Done here, at migration time, rather than being left to the next `set`: one atomic
/// rewrite is a thing that can be tested and reasoned about, where "the file is wrong until
/// the user happens to change a setting" is a second state the whole product would have to
/// keep working in.
///
/// Under the preferences lock, and re-reading the file inside it, because this runs on the
/// *load* path: `set` and `glass.reset` hold that lock across their own read-modify-write,
/// and a rewrite computed from a read taken before one of them ran would land after it and
/// undo the setting it just wrote. Taken non-blocking, and skipped when it is held, because
/// the load path may never wait on that lock -- `set lock.*` restarts hypridle while holding
/// it, and hypridle's `ExecStartPre` re-enters this binary to render, so a blocking acquire
/// here would deadlock that restart. Skipping is safe: whoever holds the lock is a writer,
/// and every writer now emits departures only. See [`PrefLock::try_acquire`].
///
/// Returns the compacted stored document so the caller's load continues from what was
/// written rather than from what was read -- which is also what keeps a dropped unknown key
/// from being reported a second time by a save later in the same process. `None` means
/// nothing was rewritten and the caller should keep the document it already has.
///
/// # Errors
///
/// Only what the Python does not catch: [`PrefsError::Unreadable`] if the re-read fails, and
/// [`PrefsError::Emit`] if the document holds a value `dump_toml()` refuses. Every `OSError`
/// -- a read-only config directory, a lock file that cannot be opened, a write that fails --
/// is `Ok(None)` instead. The load has to survive those: the effective configuration is
/// correct either way, and the file keeps its old shape until a write can happen.
pub fn compact_preferences_file(
    paths: &Paths,
    defaults: &Defaults,
    sink: Option<&mut Vec<String>>,
) -> Result<Option<toml::Table>, PrefsError> {
    // Nothing to shrink, and nothing to create: a fresh install has no layer 2 at all, and
    // writing a stamp-only file here would both invent a user-owned file from a read path
    // and take the bootstrap's GPU gate -- which writes preferences.toml only when it does
    // not already exist -- out of play.
    if !paths.host.preferences.exists() {
        return Ok(None);
    }
    let Ok(Some(_lock)) = PrefLock::try_acquire(paths) else {
        return Ok(None);
    };
    let mut stored = load_toml(&paths.host.preferences)?;
    migrate_preference_values(&mut stored);
    let document = preference_document(&stored, defaults.values(), sink);
    // Only when the rewrite would actually change something. A file that is already
    // departures-only -- the two keys the bootstrap's GPU gate writes, or a hand-written file
    // -- is left exactly as it is, comments and all, because the emitter cannot carry a
    // comment and rewriting it would cost the user the explanation of why the value is
    // there. Its stamp then stays behind, and this step re-runs on every load: a table diff
    // over ~50 keys, and no write.
    if py_equal_table(
        &preference_sections(&document),
        &preference_sections(&stored),
    ) {
        return Ok(None);
    }
    let text = emit_document(&document)?;
    match atomic_write(&paths.host.preferences, &text) {
        Ok(()) => Ok(Some(document)),
        Err(_) => Ok(None),
    }
}

/// Bring a stored `preferences.toml` up to [`PREFERENCES_VERSION`].
///
/// Two kinds of step, gated on the same stamp: the value renames, which happen in memory and
/// reach the file whenever it is next written, and v5, which rewrites the file itself.
///
/// # Errors
///
/// Whatever [`compact_preferences_file`] propagates.
pub fn migrate_preferences(
    paths: &Paths,
    stored: toml::Table,
    defaults: &Defaults,
    sink: Option<&mut Vec<String>>,
) -> Result<toml::Table, PrefsError> {
    let version = schema_version(&stored);
    if version >= PREFERENCES_VERSION {
        return Ok(stored);
    }
    let mut stored = stored;
    migrate_preference_values(&mut stored);
    if version < 5 {
        if let Some(compacted) = compact_preferences_file(paths, defaults, sink)? {
            return Ok(compacted);
        }
    }
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::{migrate_preference_values, schema_version, PREFERENCES_VERSION, WALLPAPER_SPLIT};

    fn table(text: &str) -> toml::Table {
        text.parse().unwrap()
    }

    fn migrated(text: &str) -> toml::Table {
        let mut stored = table(text);
        migrate_preference_values(&mut stored);
        stored
    }

    #[test]
    fn a_file_with_no_stamp_reads_as_version_one() {
        assert_eq!(schema_version(&toml::Table::new()), 1);
        assert_eq!(schema_version(&table("[schema]\n")), 1);
        assert_eq!(schema_version(&table("schema = 5\n")), 1);
        assert_eq!(
            schema_version(&table("[schema]\npreferences_version = 4\n")),
            4
        );
    }

    /// A hand-mangled stamp is older than current rather than fatal, and a bool is not an
    /// int however much Python's type system says otherwise.
    #[test]
    fn a_mangled_stamp_reads_as_version_one() {
        for text in [
            "[schema]\npreferences_version = \"5\"\n",
            "[schema]\npreferences_version = true\n",
            "[schema]\npreferences_version = 5.0\n",
        ] {
            assert_eq!(schema_version(&table(text)), 1, "{text}");
        }
        assert_eq!(
            schema_version(&table("[schema]\npreferences_version = 99\n")),
            99
        );
    }

    #[test]
    fn v2_renames_the_corner_radius_scale_once() {
        let stored = migrated("[appearance]\ncorner_radius = \"small\"\n");
        assert_eq!(
            stored,
            table("[appearance]\ncorner_radius = \"normal\"\n[schema]\npreferences_version = 5\n")
        );
        // Stamped past the rename, so it does not fire again -- which is what stops the
        // setting walking from normal to large on every load.
        let stored = migrated(
            "[schema]\npreferences_version = 3\n[appearance]\ncorner_radius = \"normal\"\n",
        );
        assert_eq!(
            stored.get("appearance").and_then(toml::Value::as_table),
            Some(&table("corner_radius = \"normal\"\n"))
        );
    }

    /// The lookup is `str(value)`, so nothing that is not a spelling can match one.
    #[test]
    fn a_corner_radius_that_is_not_a_spelling_is_left_alone() {
        let stored = migrated("[appearance]\ncorner_radius = 7\n");
        assert_eq!(
            stored.get("appearance").and_then(toml::Value::as_table),
            Some(&table("corner_radius = 7\n"))
        );
    }

    #[test]
    fn v3_gives_each_appearance_its_own_wallpaper() {
        let stored = migrated(
            "[appearance]\nwallpaper = \"/pic.png\"\nwallpaper_source = \"color\"\n\
             wallpaper_color = \"#010203\"\n",
        );
        let appearance = stored
            .get("appearance")
            .and_then(toml::Value::as_table)
            .unwrap();
        for (legacy, _) in WALLPAPER_SPLIT {
            assert!(
                !appearance.contains_key(legacy),
                "{legacy} was carried across"
            );
        }
        assert_eq!(appearance.len(), 6);
        for scheme in ["light", "dark"] {
            assert_eq!(
                appearance.get(&format!("wallpaper_{scheme}")),
                Some(&toml::Value::String("/pic.png".to_owned()))
            );
        }
    }

    /// A deliberate choice outranks the value it was split from.
    #[test]
    fn a_half_that_is_already_set_survives_the_split() {
        let stored = migrated(
            "[schema]\npreferences_version = 2\n[appearance]\n\
             wallpaper = \"/pic.png\"\nwallpaper_dark = \"/kept.png\"\n",
        );
        let appearance = stored
            .get("appearance")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(
            appearance.get("wallpaper_dark"),
            Some(&toml::Value::String("/kept.png".to_owned()))
        );
        assert_eq!(
            appearance.get("wallpaper_light"),
            Some(&toml::Value::String("/pic.png".to_owned()))
        );
    }

    #[test]
    fn a_current_file_is_returned_untouched() {
        let text = "[schema]\npreferences_version = 5\n[appearance]\ncorner_radius = \"small\"\n";
        assert_eq!(migrated(text), table(text));
    }

    #[test]
    fn the_stamp_is_written_even_when_no_value_moved() {
        let stored = migrated("[appearance]\naccent_color = \"red\"\n");
        assert_eq!(
            schema_version(&stored),
            PREFERENCES_VERSION,
            "the in-memory table carries the stamp the file will get"
        );
    }

    /// The Python raises `TypeError` here and takes the process with it; see the parity
    /// gap on `migrate_preference_values`.
    #[test]
    fn a_top_level_schema_that_is_not_a_table_is_left_without_a_stamp() {
        let stored = migrated("schema = 5\n[appearance]\naccent_color = \"red\"\n");
        assert_eq!(stored.get("schema"), Some(&toml::Value::Integer(5)));
    }
}
