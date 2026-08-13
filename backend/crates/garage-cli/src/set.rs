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
//!   user's choice on disk. A compositor that is not up, a `systemctl` that refuses, a
//!   wallpaper that has been deleted -- all of them are reported *after* the choice is
//!   recorded, which is the only order in which a refusal cannot cost the user their setting.

use garage_apply::cx::SessionCx;
use garage_apply::route::apply_changed_preference;
use garage_apply::snapshot::preferences_json;
use garage_core::paths::Paths;
use garage_core::schema::{Notes, PreferenceKey, Preferences};
use garage_core::traits::Runner;
use garage_prefs::{load_effective, report_preference_notes, save_preferences, PrefLock};
use garage_proc::{Hyprctl, Luac};
use garage_render::cx::RenderCx;
use serde_json::Value;

use crate::error::CliError;

/// `set KEY JSON_VALUE`, from the argument-count guard to the payload the envelope carries.
///
/// # Errors
///
/// [`CliError::SetUsage`] for the wrong number of arguments, then whatever the load, the
/// schema, the save or the route walk refuses. Every one of them reaches the envelope as its
/// own `Display`.
pub(crate) fn set(paths: &Paths, proc: &dyn Runner, argv: &[String]) -> Result<Value, CliError> {
    if argv.len() != 4 {
        return Err(CliError::SetUsage);
    }
    let (Some(key_text), Some(value_text)) = (argv.get(2), argv.get(3)) else {
        return Err(CliError::SetUsage);
    };
    let lock = PrefLock::acquire(paths)?;
    let effective = load_effective(paths, None)?;
    let mut config = effective.preferences;
    let raw: Value = serde_json::from_str(value_text)?;
    let key: PreferenceKey = key_text.parse()?;
    let mut notes = Notes::new();
    // The Python stores the raw value and lets `validate_preferences()` correct it on the
    // next line; `Preferences::set` fuses the two and produces the same note, which goes to
    // stderr at the moment it is produced exactly as the Python's sink does.
    config.set(key, &json_value(&raw), &mut notes)?;
    report_preference_notes(notes.as_slice(), None);
    save_preferences(paths, &config, &lock, None)?;
    walk(key, paths, proc, &config)?;
    // Explicit, and at the end rather than at the save: the lock covers the apply too.
    drop(lock);
    Ok(preferences_json(&config, &effective.schema))
}

/// `for step in PREFERENCE_ROUTES[route]: globals()[name](config, *arguments)`, through
/// [`apply_changed_preference`] -- which lives in `garage-apply` because `action
/// appearance.night_shift.toggle` walks the very same route from there, with no `main()`
/// branch between it and the step table.
///
/// The contexts are built here and nowhere else on this path, which is what keeps the render
/// half unable to reach the lock: [`RenderCx`] carries no runner and no lock, and the
/// [`SessionCx`] that does carry a runner is what the walk is handed.
fn walk(
    key: PreferenceKey,
    paths: &Paths,
    proc: &dyn Runner,
    config: &Preferences,
) -> Result<(), CliError> {
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let render = RenderCx::new(config, paths, &monitors, &lua);
    let mut session = SessionCx::new(render, proc);
    Ok(apply_changed_preference(&mut session, key)?)
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

    use super::{json_value, set};
    use crate::testing::Offline;
    use garage_apply::route::route_for;
    use garage_apply::snapshot::preferences_json;

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

    /// The whole flow, end to end: the lock is taken, layer 2 is loaded, the departure is
    /// written, and the route is walked against an offline machine. The file assertion is the
    /// load-bearing half -- a port that walked the route *before* writing would lose the
    /// user's choice if a step refused, and the envelope alone cannot tell the two apart.
    #[test]
    fn a_set_writes_the_departure_and_then_walks_its_route() {
        let (home, paths) = scratch("set-theme-mode");
        let proc = Offline::new();
        let payload = set(
            &paths,
            &proc,
            &argv(&["set", "appearance.theme_mode", "\"light\""]),
        )
        .expect("the whole route walks against an offline machine");
        assert_eq!(
            payload.pointer("/appearance/theme_mode"),
            Some(&Value::String("light".to_owned()))
        );
        assert_eq!(
            fs::read_to_string(&paths.host.preferences).expect("the file was written"),
            "[schema]\npreferences_version = 5\n\n[appearance]\ntheme_mode = \"light\"\n"
        );
        // Route::Theme is apply_theme_if_scheme_moved, and nothing has ever been pushed on
        // this scratch machine, so the gate is open and the palette is pushed for real.
        assert!(proc.calls().contains(
            &"gsettings set org.gnome.desktop.interface color-scheme prefer-light".to_owned()
        ));
        assert_eq!(
            proc.calls().last().map(String::as_str),
            Some("hyprctl reload")
        );
        drop(fs::remove_dir_all(&home));
    }

    /// A value the schema refuses is corrected rather than refused, and the corrected value
    /// equals the shipped default -- so the departures walk drops it and the file carries the
    /// stamp and nothing else.
    #[test]
    fn a_refused_value_is_corrected_and_leaves_no_departure_behind() {
        let (home, paths) = scratch("set-bad-enum");
        set(
            &paths,
            &Offline::new(),
            &argv(&["set", "appearance.theme_mode", "\"sideways\""]),
        )
        .expect("a refused value is corrected rather than refused");
        assert_eq!(
            fs::read_to_string(&paths.host.preferences).expect("the file was written"),
            "[schema]\npreferences_version = 5\n"
        );
        drop(fs::remove_dir_all(&home));
    }

    #[test]
    fn a_key_the_schema_does_not_have_is_refused_before_anything_is_written() {
        let (home, paths) = scratch("set-unknown-key");
        let error = set(
            &paths,
            &Offline::new(),
            &argv(&["set", "nonesuch.key", "\"x\""]),
        )
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
        let error = set(
            &paths,
            &Offline::new(),
            &argv(&["set", "nonesuch.key", "{not json"]),
        )
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
    fn the_envelope_payload_is_grouped_by_section_in_declaration_order() {
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let document = preferences_json(defaults.values(), &toml::Table::new());
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
