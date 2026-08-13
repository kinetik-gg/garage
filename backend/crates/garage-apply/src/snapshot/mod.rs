//! `make_snapshot()`: the whole live state the QML client draws every pane from, in one call.
//!
//! One JSON object, assembled from the preferences plus eight live reads: the resolved
//! default applications, the display arrangement, the workspace allocation over it, the
//! resolved shortcut set, audio, the date and time, the region, and whether a touchpad is
//! attached -- plus a capability map (which of `hyprctl`, `hyprpaper`, `hyprsunset`, `pactl`
//! and `hypridle` are even on the machine) and an error string carrying whatever
//! `load_preferences()` could not read.
//!
//! Preferences are reported exactly as stored, not with the wallpaper resolved to whatever
//! `current` points at -- that used to happen, and it meant the pane was shown a path nothing
//! had ever written: after picking a solid colour it read back the generated
//! `solid-1c1c1e.png` as though that were the chosen picture.
//!
//! See the submodules for each live section: [`display`], [`workspaces`], [`audio`],
//! [`input`], [`apps`], [`datetime`] and [`region`] are the reads `make_snapshot()` composes;
//! [`keybindings`] is the resolved shortcut set alongside them. All doc-only: every function
//! here returns a snapshot value for the JSON envelope, not a `Result<(), ApplyError>`, and
//! none of them is reached through `Route::steps()` -- `snapshot` is its own top-level
//! command in `main()`, not a route.

mod apps;
mod value;

pub use value::preferences_json;
mod audio;
mod datetime;
pub(crate) mod display;
mod input;
mod keybindings;
mod region;
mod workspaces;

use garage_core::paths::Paths;
use garage_core::schema::defaults::Defaults;
use garage_core::traits::Runner;
use garage_prefs::load_effective;
use garage_proc::{which, Hyprctl, Luac};
use garage_render::cx::RenderCx;
use serde_json::{Map, Value};

use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::keybind::load_keybindings;
use crate::snapshot::value::entries_value;

/// The tools the pane greys a control out for when they are missing (garage:5150-5152).
const CAPABILITIES: [&str; 5] = ["hyprctl", "hyprpaper", "hyprsunset", "pactl", "hypridle"];

/// `make_snapshot()` (garage:5129-5155): the whole live state, in one JSON object.
///
/// Takes `paths` and a runner rather than a built context, for the same reason
/// [`crate::actions::action`] does and one more: the load is allowed to *fail* here. A
/// `preferences.toml` that cannot be parsed degrades to the compiled-in defaults with the
/// refusal carried in the `error` field, so the pane comes up able to say what is wrong
/// instead of not coming up at all -- and a context built before the load could not express
/// that.
///
/// # Errors
///
/// [`ApplyError::Render`] from `workspaces_snapshot()`'s allocator, which is the one read
/// here that can write a file and therefore the one that can fail. Everything else degrades:
/// `json_command()` folds every way of not getting an answer into an empty one, and
/// `display_snapshot()` treats an unreadable `displays.toml` as an empty layout.
pub fn make_snapshot(paths: &Paths, proc: &dyn Runner) -> Result<Value, ApplyError> {
    let (config, stamp, error) = match load_effective(paths, None) {
        Ok(effective) => (effective.preferences, effective.schema, String::new()),
        // `copy.deepcopy(FALLBACK_DEFAULTS)`: the compiled-in table, which carries no stamp
        // section either, so the envelope's `preferences` has no `[schema]` in this arm.
        Err(refusal) => (
            Defaults::compiled()
                .map_err(|failure| ApplyError::Settings(failure.to_string()))?
                .values()
                .clone(),
            toml::Table::new(),
            refusal.to_string(),
        ),
    };
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let render = RenderCx::new(&config, paths, &monitors, &lua);
    let cx = SessionCx::new(render, proc);
    assemble(&cx, &stamp, &error)
}

/// The object itself, in the Python's own insertion order -- which is what the envelope
/// prints, so it is a contract and not a detail.
fn assemble(cx: &SessionCx<'_>, stamp: &toml::Table, error: &str) -> Result<Value, ApplyError> {
    let paths = cx.render().paths();
    let primary = std::env::var("HYPR_PRIMARY_MONITOR").unwrap_or_default();
    let displays = display::display_snapshot(cx, &primary);
    let mut document = Map::new();
    // Reported exactly as stored, not with the wallpaper resolved to whatever `current`
    // points at -- that used to happen, and it meant the pane was shown a path nothing had
    // ever written: after picking a solid colour it read back the generated `solid-1c1c1e.png`
    // as though that were the chosen picture.
    document.insert(
        "preferences".to_owned(),
        preferences_json(cx.render().prefs(), stamp),
    );
    document.insert("defaultApps".to_owned(), apps::default_apps_snapshot(cx));
    document.insert("displays".to_owned(), entries_value(&displays));
    document.insert(
        "workspaces".to_owned(),
        workspaces::workspaces_snapshot(cx, &displays, &primary)?,
    );
    document.insert(
        "keybindings".to_owned(),
        keybindings::keybindings_snapshot(cx, &load_keybindings(paths, None)),
    );
    document.insert("audio".to_owned(), audio::audio_snapshot(cx));
    document.insert("dateTime".to_owned(), datetime::datetime_snapshot(cx));
    document.insert("region".to_owned(), region::region_snapshot(cx));
    document.insert("inputCapabilities".to_owned(), input::input_snapshot(cx));
    document.insert("capabilities".to_owned(), capabilities());
    document.insert("error".to_owned(), Value::String(error.to_owned()));
    Ok(Value::Object(document))
}

/// `{name: shutil.which(name) is not None for name in (...)}`.
fn capabilities() -> Value {
    let mut found = Map::new();
    for name in CAPABILITIES {
        found.insert(name.to_owned(), Value::Bool(which(name).is_some()));
    }
    Value::Object(found)
}
