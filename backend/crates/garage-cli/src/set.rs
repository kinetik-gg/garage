//! `main()`'s `set` branch: one preference written, and just that change applied.
//!
//! The whole branch runs under `PREFERENCES_LOCK`, and the Python says why: "One process per
//! wheel tick: the UI's sliders fire a set per notch, so several of these run at once. Each
//! reads the whole file and rewrites it whole, and unserialised the last writer wins with a
//! copy that predates its neighbours' keys. Applying under the same lock keeps the live
//! state in step with the file rather than letting a stale apply land last." So the lock is
//! taken before the load and dropped after the *apply*, not after the save.
//!
//! Order is behaviour here, twice over:
//!
//! * the JSON value is parsed before the key is, because `set_nested(config, argv[2],
//!   json.loads(argv[3]))` evaluates its arguments before it runs -- a malformed value is
//!   reported ahead of an unknown key, not the other way round;
//! * the file is written before the route is walked, so a route that fails still leaves the
//!   user's choice on disk. That is what makes today's half-ported state legible rather than
//!   lossy: `set appearance.theme_mode '"dark"'` records the departure and then says which
//!   applier is still owed.

use garage_apply::cx::SessionCx;
use garage_apply::dispatch::run_apply;
use garage_core::paths::Paths;
use garage_core::schema::{Notes, PreferenceKey, Preferences, Route, Section, Step};
use garage_prefs::{load_preferences, report_preference_notes, save_preferences, PrefLock};
use garage_proc::{Hyprctl, Luac, System};
use garage_render::cx::RenderCx;
use garage_render::dispatch::run_render;
use serde_json::{Map, Value};

use crate::error::CliError;

/// `set KEY JSON_VALUE`, from the argument-count guard to the payload the envelope carries.
///
/// # Errors
///
/// [`CliError::SetUsage`] for the wrong number of arguments, then whatever the load, the
/// schema, the save or the route walk refuses. Every one of them reaches the envelope as its
/// own `Display`.
pub(crate) fn set(paths: &Paths, argv: &[String]) -> Result<Value, CliError> {
    if argv.len() != 4 {
        return Err(CliError::SetUsage);
    }
    let (Some(key_text), Some(value_text)) = (argv.get(2), argv.get(3)) else {
        return Err(CliError::SetUsage);
    };
    let lock = PrefLock::acquire(paths)?;
    let mut config = load_preferences(paths, None)?;
    let raw: Value = serde_json::from_str(value_text)?;
    let key: PreferenceKey = key_text.parse()?;
    let mut notes = Notes::new();
    // The Python stores the raw value and lets `validate_preferences()` correct it on the
    // next line; `Preferences::set` fuses the two and produces the same note, which goes to
    // stderr at the moment it is produced exactly as the Python's sink does.
    config.set(key, &json_value(&raw), &mut notes)?;
    report_preference_notes(notes.as_slice(), None);
    save_preferences(paths, &config, &lock, None)?;
    walk(route_for(key)?, paths, &config)?;
    // Explicit, and at the end rather than at the save: the lock covers the apply too.
    drop(lock);
    Ok(preferences_json(&config))
}

/// `apply_changed_preference()`'s dispatch, minus the walk: which route one changed key takes.
///
/// The two refusals below cannot be reached from a parsed [`PreferenceKey`] today, and they
/// are written down anyway because the Python's own fallback cannot be reached either -- a
/// key that is in `PREFERENCE_SCHEMA` always has a `route`, and a key that is not never
/// survives `set_nested()`. What `SECTION_ROUTES` and these two messages describe is the
/// behaviour a key *outside* the schema has always had, and the Python's comment is explicit
/// that appearance, general and bar name the key while the rest name the section. Dropping
/// either half here would be a refactor inventing an error the product never had.
///
/// # Errors
///
/// [`CliError::UnsupportedPreference`] or [`CliError::UnsupportedSection`] for a key that
/// routes nowhere, chosen by the same three-section split the Python makes.
pub(crate) fn route_for(key: PreferenceKey) -> Result<Route, CliError> {
    if let Some(route) = key.route() {
        return Ok(route);
    }
    let section = key.section();
    if let Some(route) = section.route() {
        return Ok(route);
    }
    match section {
        Section::Appearance | Section::General | Section::Bar => {
            Err(CliError::UnsupportedPreference {
                section,
                key: key.name().to_owned(),
            })
        }
        Section::Indexing
        | Section::Input
        | Section::Lock
        | Section::Region
        | Section::Workspaces => Err(CliError::UnsupportedSection(section)),
    }
}

/// `for step in PREFERENCE_ROUTES[route]: globals()[name](config, *arguments)`.
///
/// The contexts are built here and nowhere else on this path, which is what keeps the
/// render half unable to reach the lock: [`RenderCx`] carries no runner and no lock, and
/// the [`SessionCx`] that does carry a runner is only ever handed to an
/// [`Step::Apply`] step.
fn walk(route: Route, paths: &Paths, config: &Preferences) -> Result<(), CliError> {
    let system = System;
    let monitors = Hyprctl::new(&system);
    let lua = Luac::new(&system);
    let render = RenderCx::new(config, paths, &monitors, &lua);
    let mut session = SessionCx::new(render, &system);
    for step in route.steps() {
        match *step {
            Step::Render(step) => run_render(step, &render)?,
            Step::Apply(step) => run_apply(step, &mut session)?,
        }
    }
    Ok(())
}

/// The whole effective configuration as `response(config)` prints it.
///
/// Sections and keys come out in declaration order, which is `preferences.defaults.toml`'s
/// own order, which is the order the Python's `deep_merge` leaves in the dict it answers
/// with -- so a client reading the two backends' envelopes side by side sees the same
/// document, not a re-sorted one.
///
/// **Parity gap, stated plainly:** the Python's `config` is a dict and can still be carrying
/// keys the schema does not have (a stamped file's unknown key survives the load) plus the
/// `[schema]` version stamp itself. A [`Preferences`] can hold neither, so both are absent
/// here. Nothing reads them off this envelope -- the QML client asks for keys it knows.
fn preferences_json(config: &Preferences) -> Value {
    let mut document = Map::new();
    config.each_key(|key, value| {
        let section = document
            .entry(key.section().as_str().to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(table) = section.as_object_mut() {
            table.insert(key.name().to_owned(), toml_value(&value));
        }
    });
    Value::Object(document)
}

/// `json.loads()`'s result, as the schema's coercion pass wants to see it.
///
/// **The one value with no counterpart, and what is done about it:** JSON has `null` and
/// TOML has nothing at all. Python's `set_nested` stores `None` and `validate_preferences`
/// then corrects it, so `garage set appearance.border_size null` is a *note*, not a refusal.
/// An empty array is the value that behaves identically through every kind in the table:
/// both unchecked kinds read it through Python truthiness, where an empty container and
/// `None` are both false, and every checked kind refuses an array exactly as it refuses
/// `None`. The one thing that differs is the note's own wording -- `[]` where Python writes
/// `None` -- which is printed once, on stderr, for a caller who sent a null on purpose.
fn json_value(value: &Value) -> toml::Value {
    match value {
        Value::Null => toml::Value::Array(Vec::new()),
        Value::Bool(flag) => toml::Value::Boolean(*flag),
        Value::Number(number) => number.as_i64().map_or_else(
            || toml::Value::Float(number.as_f64().unwrap_or_default()),
            toml::Value::Integer,
        ),
        Value::String(text) => toml::Value::String(text.clone()),
        Value::Array(items) => toml::Value::Array(items.iter().map(json_value).collect()),
        Value::Object(map) => toml::Value::Table(
            map.iter()
                .map(|(key, item)| (key.clone(), json_value(item)))
                .collect(),
        ),
    }
}

/// The other direction, for the envelope. A TOML datetime cannot reach a [`Preferences`]
/// field -- no kind in the table stores one -- so it is spelled the way TOML spells it
/// rather than given a shape of its own.
fn toml_value(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(text) => Value::String(text.clone()),
        toml::Value::Integer(number) => Value::from(*number),
        toml::Value::Float(number) => Value::from(*number),
        toml::Value::Boolean(flag) => Value::Bool(*flag),
        toml::Value::Datetime(stamp) => Value::String(stamp.to_string()),
        toml::Value::Array(items) => Value::Array(items.iter().map(toml_value).collect()),
        toml::Value::Table(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), toml_value(item)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::schema::{PreferenceKey, Route};
    use serde_json::{json, Value};

    use super::{json_value, preferences_json, route_for, set, toml_value};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    /// The shipped defaults where a stowed machine keeps them: a *symlink into the
    /// checkout*, not a copy. Same shape the differential harness plants, and for the same
    /// reason -- a writer that followed and truncated the link would look correct against a
    /// copy while editing the developer's tree in the field.
    fn scratch(label: &str) -> (PathBuf, Paths) {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let home = std::env::temp_dir().join(format!(
            "garage-cli-{label}-{}-{serial}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&home));
        fs::create_dir_all(home.join(".config/garage")).expect("scratch is creatable");
        fs::create_dir_all(home.join(".local/state/garage")).expect("scratch is creatable");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../desktop/.config/garage/preferences.defaults.toml");
        std::os::unix::fs::symlink(
            &source,
            home.join(".config/garage/preferences.defaults.toml"),
        )
        .expect("the defaults link is plantable");
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        let paths = Paths::from_env_map(&env);
        (home, paths)
    }

    fn argv(parts: &[&str]) -> Vec<String> {
        std::iter::once("garage")
            .chain(parts.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// The happy flow as far as it goes today: the lock is taken, layer 2 is loaded, the
    /// departure is written, and only then does the route say which applier is owed. The
    /// file assertion is the load-bearing half -- a port that reported the missing applier
    /// *before* writing would lose the user's choice, and the envelope alone cannot tell the
    /// two apart.
    #[test]
    fn a_set_writes_the_departure_and_then_names_the_applier_that_is_owed() {
        let (home, paths) = scratch("set-theme-mode");
        let error = set(
            &paths,
            &argv(&["set", "appearance.theme_mode", "\"light\""]),
        )
        .expect_err("apply_theme_if_scheme_moved is still a stub");
        assert_eq!(
            error.to_string(),
            "apply_theme_if_scheme_moved has not been ported yet"
        );
        assert_eq!(
            fs::read_to_string(&paths.host.preferences).expect("the file was written"),
            "[schema]\npreferences_version = 5\n\n[appearance]\ntheme_mode = \"light\"\n"
        );
        drop(fs::remove_dir_all(&home));
    }

    /// A value the schema refuses is corrected rather than refused, and the corrected value
    /// equals the shipped default -- so the departures walk drops it and the file carries the
    /// stamp and nothing else.
    #[test]
    fn a_refused_value_is_corrected_and_leaves_no_departure_behind() {
        let (home, paths) = scratch("set-bad-enum");
        let error = set(
            &paths,
            &argv(&["set", "appearance.theme_mode", "\"sideways\""]),
        )
        .expect_err("apply_theme_if_scheme_moved is still a stub");
        assert_eq!(
            error.to_string(),
            "apply_theme_if_scheme_moved has not been ported yet"
        );
        assert_eq!(
            fs::read_to_string(&paths.host.preferences).expect("the file was written"),
            "[schema]\npreferences_version = 5\n"
        );
        drop(fs::remove_dir_all(&home));
    }

    #[test]
    fn a_key_the_schema_does_not_have_is_refused_before_anything_is_written() {
        let (home, paths) = scratch("set-unknown-key");
        let error = set(&paths, &argv(&["set", "nonesuch.key", "\"x\""]))
            .expect_err("the schema gates `set`");
        assert_eq!(error.to_string(), "Unknown preference: nonesuch.key");
        assert!(!paths.host.preferences.exists());
        drop(fs::remove_dir_all(&home));
    }

    #[test]
    fn a_malformed_value_is_reported_ahead_of_an_unknown_key() {
        // Argument evaluation order, made a test: `json.loads(argv[3])` runs before
        // `set_nested` does, so this is a JSON complaint and not "Unknown preference".
        let (home, paths) = scratch("set-bad-json");
        let error = set(&paths, &argv(&["set", "nonesuch.key", "{not json"]))
            .expect_err("the value does not parse");
        assert!(
            !error.to_string().starts_with("Unknown preference"),
            "{error}"
        );
        drop(fs::remove_dir_all(&home));
    }

    #[test]
    fn every_key_the_schema_has_routes_somewhere() {
        // The precondition the two "Unsupported ..." arms are unreachable *because* of.
        // A key added without a route makes this fail rather than making `set` silently
        // write and never apply.
        for key in PreferenceKey::ALL.iter().copied() {
            assert!(route_for(key).is_ok(), "{key} routes nowhere");
        }
    }

    #[test]
    fn a_key_takes_its_own_route_rather_than_its_sections() {
        assert_eq!(route_for(PreferenceKey::ThemeMode).ok(), Some(Route::Theme));
        assert_eq!(
            route_for(PreferenceKey::LockTimeout).ok(),
            Some(Route::Idle)
        );
    }

    #[test]
    fn json_scalars_land_on_the_toml_value_the_schema_reads() {
        assert_eq!(
            json_value(&json!("dark")),
            toml::Value::String("dark".to_owned())
        );
        assert_eq!(json_value(&json!(2)), toml::Value::Integer(2));
        assert_eq!(json_value(&json!(0.65)), toml::Value::Float(0.65));
        assert_eq!(json_value(&json!(true)), toml::Value::Boolean(true));
        assert_eq!(json_value(&Value::Null), toml::Value::Array(Vec::new()));
    }

    #[test]
    fn a_json_integer_stays_an_integer_rather_than_becoming_a_float() {
        // `1 == 1.0` in Python, and the emitter spells the two differently: a UI that sends
        // JSON `0` for a float-valued key must not put `0` in the file where `0.0` belongs,
        // and the coercion pass is what reconciles them -- not this conversion.
        assert_eq!(json_value(&json!(0)), toml::Value::Integer(0));
    }

    #[test]
    fn the_two_directions_agree_on_everything_a_preference_can_hold() {
        for value in [json!("x"), json!(3), json!(1.5), json!(false)] {
            assert_eq!(toml_value(&json_value(&value)), value);
        }
    }

    #[test]
    fn the_envelope_payload_is_grouped_by_section_in_declaration_order() {
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let document = preferences_json(defaults.values());
        let sections: Vec<&str> = document
            .as_object()
            .map(|map| map.keys().map(String::as_str).collect())
            .unwrap_or_default();
        assert_eq!(sections.first().copied(), Some("appearance"));
        let first_key = document
            .get("appearance")
            .and_then(Value::as_object)
            .and_then(|table| table.keys().next())
            .map(String::as_str);
        assert_eq!(first_key, Some("wallpaper_light"));
        // Every section the schema declares is present, and nothing else is.
        assert_eq!(sections.len(), garage_core::schema::Section::ALL.len());
    }
}
