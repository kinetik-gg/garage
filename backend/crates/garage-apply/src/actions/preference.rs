//! The two actions that are read-modify-writes of `preferences.toml`, under the lock.
//!
//! `toggle_boolean_preference()` (garage:5331-5344) atomically inverts one boolean and walks
//! its ordinary route -- the pane's night shift switch is the only caller, and it fires it
//! rather than a `set` because it does not know the current value and must not race a second
//! toggle into reading the same one twice.
//!
//! `glass_reset()` (garage:5359-5388) walks every material preference back to its shipped
//! default. It needs no list of its own to keep in step with the schema, because every
//! material preference is named with the `glass_` prefix -- so a new one is covered the
//! moment it is declared, which is the property a hand-maintained list would lose.
//!
//! Both block on `PREFERENCES_LOCK`, and that is the deliberate difference from the load
//! path, which only ever *tries* it: nothing either of these does restarts a service whose
//! `ExecStartPre` re-enters this binary, so there is no lock this can be waiting behind that
//! is waiting behind it. The load path cannot say that -- see `compact_preferences_file()` --
//! which is why it takes the lock optimistically and carries on without it.

use garage_core::paths::Paths;
use garage_core::schema::{Notes, PreferenceKey, Section};
use garage_core::traits::Runner;
use garage_prefs::{load_preferences, save_preferences, shipped_defaults, PrefLock};
use garage_proc::{Hyprctl, Luac};
use garage_render::cx::RenderCx;
use garage_render::render_preferences;

use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::glass::apply_glass;
use crate::route::apply_changed_preference;

/// Atomically invert one boolean preference and apply its normal route (garage:5331-5343).
///
/// Reports the value it settled on, which is the Python's return: nothing in `action()` reads
/// it today, and it is kept because the function's whole point is that the caller could not
/// have known it in advance.
///
/// # Errors
///
/// [`ApplyError::Lock`] if the preferences lock cannot be taken, [`ApplyError::Prefs`] if
/// layer 2 cannot be read or written, [`ApplyError::Set`] if the schema refuses the write,
/// and whatever the route walk refuses.
pub(crate) fn toggle_boolean_preference(
    paths: &Paths,
    proc: &dyn Runner,
    key: PreferenceKey,
) -> Result<bool, ApplyError> {
    let lock = PrefLock::acquire(paths)?;
    let mut config = load_preferences(paths, None)?;
    // `not bool(config[section][key])`: Python truthiness of whatever is stored, which for a
    // key declared "unchecked" can be a string a hand edit put there.
    let next = !py_truthy(&config.get(key));
    let mut notes = Notes::new();
    config.set(key, &toml::Value::Boolean(next), &mut notes)?;
    save_preferences(paths, &config, &lock, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let render = RenderCx::new(&config, paths, &monitors, &lua);
    let mut session = SessionCx::new(render, proc);
    apply_changed_preference(&mut session, key)?;
    drop(lock);
    Ok(next)
}

/// Walk every `glass_*` preference back to its shipped default, then push the result
/// (garage:5359-5388).
///
/// # Errors
///
/// The same set [`toggle_boolean_preference`] can raise, plus [`ApplyError::Render`] if the
/// fragment could not be rewritten and whatever [`apply_glass`] refuses.
pub(crate) fn glass_reset(paths: &Paths, proc: &dyn Runner) -> Result<(), ApplyError> {
    let lock = PrefLock::acquire(paths)?;
    let mut config = load_preferences(paths, None)?;
    let defaults = shipped_defaults(paths)?;
    let mut notes = Notes::new();
    for key in material_keys() {
        config.set(key, &defaults.values().get(key), &mut notes)?;
    }
    save_preferences(paths, &config, &lock, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let render = RenderCx::new(&config, paths, &monitors, &lua);
    let mut session = SessionCx::new(render, proc);
    render_preferences(&render)?;
    // The same live push a single slider takes, rather than the full reload this used to do:
    // apply_glass now covers the core decoration options too, and falls back to a reload
    // itself if the eval cannot land. Inside the lock for the same reason `set` applies
    // inside it: a stale apply landing last would leave the desktop disagreeing with the file.
    apply_glass(&mut session)?;
    drop(lock);
    Ok(())
}

/// `for key, fallback in defaults["appearance"].items(): if key.startswith("glass_")`.
fn material_keys() -> impl Iterator<Item = PreferenceKey> {
    PreferenceKey::ALL
        .iter()
        .copied()
        .filter(|key| key.section() == Section::Appearance && key.name().starts_with("glass_"))
}

/// `bool(value)` for a stored TOML scalar, which is Python truthiness and not TOML's.
fn py_truthy(value: &toml::Value) -> bool {
    match value {
        toml::Value::Boolean(flag) => *flag,
        toml::Value::Integer(number) => *number != 0,
        toml::Value::Float(number) => *number != 0.0,
        toml::Value::String(text) => !text.is_empty(),
        toml::Value::Array(items) => !items.is_empty(),
        toml::Value::Table(entries) => !entries.is_empty(),
        // A TOML datetime is an object, and every object is truthy in Python.
        toml::Value::Datetime(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::material_keys;
    use garage_core::schema::PreferenceKey;

    #[test]
    fn every_material_preference_is_covered_by_the_prefix_and_nothing_else_is() {
        let covered: Vec<&str> = material_keys().map(PreferenceKey::name).collect();
        assert!(covered.contains(&"glass_mode"));
        assert!(covered.contains(&"glass_blur"));
        assert!(covered.contains(&"glass_refraction"));
        assert!(!covered.contains(&"accent_color"));
        // The claim the Python's comment makes, as a test: the prefix *is* the list.
        for key in PreferenceKey::ALL.iter().copied() {
            assert_eq!(
                key.name().starts_with("glass_"),
                covered.contains(&key.name()),
                "{key}"
            );
        }
    }
}
