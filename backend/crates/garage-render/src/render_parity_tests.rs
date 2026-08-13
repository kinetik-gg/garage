//! Byte-parity tests for [`crate::preferences::render_preferences`],
//! [`crate::idle::render_idle`], [`crate::motion::render_motion`] and
//! [`crate::accent::render_accent`] against the real Python backend.
//!
//! `testdata/render_fixtures.json` is the output of a throwaway script (not committed --
//! see this task's report) that loads `desktop/.local/bin/garage` with `SourceFileLoader`,
//! the same way `tests/harness.py` does, and for each scenario in the matrix below merges a
//! handful of departures onto `shipped_defaults()`, runs them through `validate_preferences()`
//! exactly as `load_preferences()` would, then calls `render_preferences()` (which also
//! calls `render_idle()`), `render_motion()` and `render_accent()` and reads every file
//! they wrote. The matrix: defaults; every `glass_mode` x `glass_blur` pair (9); `border_size`
//! 0 and nonzero; `reduce_motion` on/off; `animation_speed` 0.5/1.0/2.0; `night_shift` on/off;
//! each lock timeout at 0/600/900 in isolation, both all-zero and all-nonzero together; and
//! one case moving several dimensions at once, the way a real edited `preferences.toml`
//! would. 31 scenarios in total.
//!
//! Each scenario's `"toml"` field is the departures exactly as the Python's own
//! `dump_toml()` wrote them, so the Rust side builds the identical [`Preferences`] by
//! parsing that text with [`Preferences::coerce_from`] rather than re-describing the
//! scenario a second time in a different shape.

use std::collections::HashMap;
use std::path::Path;

use garage_core::paths::Paths;
use garage_core::schema::defaults::Defaults;
use garage_core::schema::notes::Notes;
use garage_core::schema::Preferences;
use garage_core::traits::{LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource};

use crate::accent::render_accent;
use crate::cx::RenderCx;
use crate::motion::render_motion;
use crate::preferences::render_preferences;

const FIXTURES: &str = include_str!("../testdata/render_fixtures.json");

struct NoMonitors;
impl MonitorSource for NoMonitors {
    fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
        Ok(vec![])
    }
}

/// Accepts every candidate, the way a machine with no `luac` installed behaves -- see
/// [`LuaSyntaxCheck`]'s own docs for why that is the correct default rather than a shortcut.
struct LuaAccepts;
impl LuaSyntaxCheck for LuaAccepts {
    fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
        Ok(())
    }
}

fn scratch_paths(label: &str) -> Paths {
    let home = std::env::temp_dir().join(format!(
        "garage-render-parity-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let env: HashMap<String, String> = [("HOME".to_owned(), home.to_string_lossy().into_owned())]
        .into_iter()
        .collect();
    Paths::from_env_map(&env)
}

fn preferences_from_toml(departures: &str) -> Preferences {
    let table: toml::Table = departures.parse().expect("fixture toml parses");
    let defaults = Defaults::compiled().expect("shipped defaults parse");
    let mut notes = Notes::new();
    let prefs = Preferences::coerce_from(&table, &defaults, &mut notes);
    assert!(
        notes.is_empty(),
        "a fixture scenario must not need coercion: {notes:?}"
    );
    prefs
}

fn fixtures() -> serde_json::Value {
    serde_json::from_str(FIXTURES).expect("testdata/render_fixtures.json is valid JSON")
}

fn field<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("fixture is missing string field {key:?}"))
}

#[test]
fn every_scenario_matches_the_python_backend_byte_for_byte() {
    let all = fixtures();
    let scenarios = all.as_object().expect("fixture root is an object");
    assert!(scenarios.len() >= 25, "the matrix should not have shrunk");

    for (name, expected) in scenarios {
        let prefs = preferences_from_toml(field(expected, "toml"));
        let paths = scratch_paths(name);
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);

        render_preferences(&cx)
            .unwrap_or_else(|error| panic!("{name}: render_preferences failed: {error}"));
        render_motion(&cx).unwrap_or_else(|error| panic!("{name}: render_motion failed: {error}"));
        render_accent(&cx).unwrap_or_else(|error| panic!("{name}: render_accent failed: {error}"));

        let fragment = std::fs::read_to_string(&paths.fragments.hyprland)
            .unwrap_or_else(|error| panic!("{name}: reading preferences.lua: {error}"));
        assert_eq!(
            fragment,
            field(expected, "preferences_lua"),
            "{name}: preferences.lua"
        );

        let idle = std::fs::read_to_string(&paths.fragments.hypridle)
            .unwrap_or_else(|error| panic!("{name}: reading hypridle.conf: {error}"));
        assert_eq!(
            idle,
            field(expected, "hypridle_conf"),
            "{name}: hypridle.conf"
        );

        let material = std::fs::read_to_string(&paths.markers.material)
            .unwrap_or_else(|error| panic!("{name}: reading the material marker: {error}"));
        assert_eq!(
            material,
            field(expected, "material"),
            "{name}: material marker"
        );

        let accent = std::fs::read_to_string(&paths.markers.accent)
            .unwrap_or_else(|error| panic!("{name}: reading the accent marker: {error}"));
        assert_eq!(accent, field(expected, "accent"), "{name}: accent marker");

        let motion = std::fs::read_to_string(&paths.markers.reduce_motion)
            .unwrap_or_else(|error| panic!("{name}: reading the reduce-motion marker: {error}"));
        assert_eq!(
            motion,
            field(expected, "reduce_motion"),
            "{name}: reduce-motion marker"
        );

        drop(std::fs::remove_dir_all(&paths.home));
    }
}

/// A `luac` that is actually installed, so this proves the generated fragment is not just
/// byte-identical to the Python's but *valid Lua* -- the one check
/// [`garage_core::fs::lua::write_lua`] itself performs in production and the fake
/// [`LuaAccepts`] above cannot stand in for.
struct RealLuac;
impl LuaSyntaxCheck for RealLuac {
    // The one place this crate is allowed to ask "is this generated fragment valid Lua" on
    // a machine that has luac -- see garage-core's `LuaSyntaxCheck` docs for why this is a
    // test-only exception, never a production path (production runs `luac` through
    // `garage_proc::run`, which this crate cannot depend on -- see `RenderCx`'s docs).
    #[allow(clippy::disallowed_methods)]
    fn check(&self, candidate: &Path) -> Result<(), LuaCheckError> {
        let output = std::process::Command::new("luac")
            .arg("-p")
            .arg(candidate)
            .output()
            .map_err(|error| LuaCheckError {
                detail: error.to_string(),
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(LuaCheckError {
                detail: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }
}

#[test]
#[allow(clippy::disallowed_methods)] // the one place this crate is allowed to ask: is this
                                     // generated fragment valid Lua, on a machine that has
                                     // luac -- see garage-core's LuaSyntaxCheck docs for why
                                     // this is a test-only exception, not a production path.
fn the_default_fragment_is_valid_lua_under_the_real_luac() {
    if std::process::Command::new("luac")
        .arg("-v")
        .output()
        .is_err()
    {
        // Missing luac is not a reason to fail -- see LuaSyntaxCheck's own docs.
        return;
    }
    let all = fixtures();
    let defaults = all
        .get("defaults")
        .expect("fixture has a defaults scenario");
    let prefs = preferences_from_toml(field(defaults, "toml"));
    let paths = scratch_paths("luac");
    let monitors = NoMonitors;
    let lua = RealLuac;
    let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);

    render_preferences(&cx).expect("the generated fragment must parse under the real luac");
    drop(std::fs::remove_dir_all(&paths.home));
}

#[test]
fn a_marker_rewrite_preserves_its_inode_and_the_lua_install_replaces_it() {
    use std::os::unix::fs::MetadataExt as _;

    let all = fixtures();
    let defaults = all
        .get("defaults")
        .expect("fixture has a defaults scenario");
    let prefs = preferences_from_toml(field(defaults, "toml"));
    let paths = scratch_paths("inodes");
    let monitors = NoMonitors;
    let lua = LuaAccepts;
    let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);

    render_accent(&cx).expect("first render_accent");
    let marker_inode_before = std::fs::metadata(&paths.markers.accent)
        .expect("marker exists")
        .ino();
    render_accent(&cx).expect("second render_accent");
    let marker_inode_after = std::fs::metadata(&paths.markers.accent)
        .expect("marker exists")
        .ino();
    assert_eq!(
        marker_inode_before, marker_inode_after,
        "a marker rewrite must keep its inode, or an inotify watch on it never fires"
    );

    render_preferences(&cx).expect("first render_preferences");
    let fragment_inode_before = std::fs::metadata(&paths.fragments.hyprland)
        .expect("fragment exists")
        .ino();
    render_preferences(&cx).expect("second render_preferences");
    let fragment_inode_after = std::fs::metadata(&paths.fragments.hyprland)
        .expect("fragment exists")
        .ino();
    assert_ne!(
        fragment_inode_before, fragment_inode_after,
        "the lua fragment install must replace the file, not edit it in place"
    );

    drop(std::fs::remove_dir_all(&paths.home));
}
