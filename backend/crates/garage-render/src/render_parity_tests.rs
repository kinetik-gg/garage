//! Byte-parity tests for [`crate::preferences::render_preferences`],
//! [`crate::idle::render_idle`], [`crate::motion::render_motion`] and
//! [`crate::accent::render_accent`] against the real Python backend.
//!
//! `testdata/render_fixtures.json` was captured during the Rust port by loading the former
//! backend with `SourceFileLoader`; for each scenario in the matrix below it merges a
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

use garage_core::schema::enums::Scheme;

use crate::accent::render_accent;
use crate::cx::RenderCx;
use crate::motion::render_motion;
use crate::palette::toolkits::render_toolkits;
use crate::preferences::render_preferences;
use crate::theme::{render_theme, resolve_theme};
use crate::wallpaper::render_wallpaper;

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

// ---------------------------------------------------------------------------
// Task 3.4b: the toolkit emitters, theme resolution and the wallpaper fragment.
// ---------------------------------------------------------------------------

/// `testdata/theme_fixtures.json` is the second fixture file, dumped the same way from the
/// same backend and kept apart from the first only because it is keyed differently: the
/// toolkit files depend on the resolved scheme and on nothing else, so a matrix over accents,
/// radii and glass modes would carry thirty-four identical copies of fifteen files. It is
/// therefore two sections. `"toolkits"` holds every file `render_toolkits()` writes, once per
/// appearance; `"scenarios"` holds the thirty-four-case matrix for the outputs that *do* vary
/// -- the resolved scheme and the marker `render_theme()` publishes -- with each case's
/// departures in the `"toml"` field exactly as `dump_toml()` wrote them.
///
/// Absolute paths are spelled against `"home"` rather than against the scratch tree the dump
/// ran in, so a comparison substitutes the running test's own home first. Two files carry one
/// -- `qt6ct.conf`'s `color_scheme_path` and `hyprpaper.conf`'s `path` -- and both are
/// genuinely absolute in production.
const THEME_FIXTURES: &str = include_str!("../testdata/theme_fixtures.json");

fn theme_fixtures() -> serde_json::Value {
    serde_json::from_str(THEME_FIXTURES).expect("testdata/theme_fixtures.json is valid JSON")
}

/// The fixture's recorded home, and the scratch home this run writes into.
fn homes(paths: &Paths, all: &serde_json::Value) -> (String, String) {
    (
        field(all, "home").to_owned(),
        paths.home.to_string_lossy().into_owned(),
    )
}

/// Every file under a directory, keyed by its path relative to that directory.
fn tree(root: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, into: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, into);
            } else if let Ok(relative) = path.strip_prefix(root) {
                into.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    found
}

#[test]
fn every_toolkit_file_matches_the_python_backend_byte_for_byte() {
    let all = theme_fixtures();
    let toolkits = all
        .get("toolkits")
        .and_then(serde_json::Value::as_object)
        .expect("the fixture has a toolkits section");
    assert_eq!(toolkits.len(), 2, "both appearances are dumped");

    for (name, expected) in toolkits {
        let scheme: Scheme = name.parse().expect("a toolkits key is a scheme name");
        let paths = scratch_paths(&format!("toolkits-{name}"));
        let (fixture_home, home) = homes(&paths, &all);
        render_toolkits(&paths, scheme)
            .unwrap_or_else(|error| panic!("{name}: render_toolkits failed: {error}"));

        let files = expected.as_object().expect("a toolkits entry is an object");
        assert!(
            files.len() >= 15,
            "{name}: the toolkit set should not shrink"
        );
        for (relative, contents) in files {
            let written = if relative == "generated/hyprlock-theme.conf" {
                paths.generated.join("hyprlock-theme.conf")
            } else {
                paths.config_home.join(relative)
            };
            let found = std::fs::read_to_string(&written)
                .unwrap_or_else(|error| panic!("{name}: reading {relative}: {error}"));
            let want = contents
                .as_str()
                .unwrap_or_else(|| panic!("{name}: {relative} is not a string"))
                .replace(&fixture_home, &home);
            assert_eq!(found, want, "{name}: {relative}");
        }
        drop(std::fs::remove_dir_all(&paths.home));
    }
}

/// The set of files, not just their contents: a write dropped from the port would otherwise
/// pass every byte comparison above by never being looked for.
#[test]
fn the_toolkit_render_writes_exactly_the_files_the_python_writes() {
    let all = theme_fixtures();
    let paths = scratch_paths("toolkit-set");

    render_toolkits(&paths, Scheme::Dark).expect("render_toolkits succeeds on a clean scratch");

    let mut expected: Vec<String> = all
        .get("toolkits")
        .and_then(|toolkits| toolkits.get("dark"))
        .and_then(serde_json::Value::as_object)
        .expect("the fixture has a dark toolkit set")
        .keys()
        .filter(|name| !name.starts_with("generated/"))
        .cloned()
        .collect();
    expected.sort();

    assert_eq!(tree(&paths.config_home), expected);
    assert_eq!(
        tree(&paths.generated),
        vec!["hyprlock-theme.conf".to_owned()]
    );
    drop(std::fs::remove_dir_all(&paths.home));
}

/// The wallpaper half of one scenario. The Python renders the fragment twice: the first write
/// reports the file as moved, the second as unchanged. That flag is the whole reason this
/// renderer returns anything rather than `()`, so both answers are pinned alongside the bytes.
fn check_wallpaper(
    name: &str,
    cx: &RenderCx<'_>,
    expected: &serde_json::Value,
    fixture_home: &str,
    home: &str,
) {
    assert!(
        render_wallpaper(cx).unwrap_or_else(|error| panic!("{name}: first render: {error}")),
        "{name}: the first wallpaper render must report a change"
    );
    assert!(
        !render_wallpaper(cx).unwrap_or_else(|error| panic!("{name}: second render: {error}")),
        "{name}: an unchanged wallpaper fragment must report no change"
    );
    let fragment = std::fs::read_to_string(&cx.paths().fragments.hyprpaper)
        .unwrap_or_else(|error| panic!("{name}: reading hyprpaper.conf: {error}"));
    assert_eq!(
        fragment,
        field(expected, "hyprpaper_conf").replace(fixture_home, home),
        "{name}: hyprpaper.conf"
    );
}

#[test]
fn every_theme_scenario_matches_the_python_backend_byte_for_byte() {
    let all = theme_fixtures();
    let scenarios = all
        .get("scenarios")
        .and_then(serde_json::Value::as_object)
        .expect("the fixture has a scenarios section");
    assert!(scenarios.len() >= 30, "the matrix should not have shrunk");

    for (name, expected) in scenarios {
        let prefs = preferences_from_toml(field(expected, "toml"));
        let paths = scratch_paths(&format!("theme-{name}"));
        let (fixture_home, home) = homes(&paths, &all);
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);

        let scheme: Scheme = field(expected, "scheme")
            .parse()
            .expect("a scenario's scheme is a scheme name");
        assert_eq!(resolve_theme(&prefs), scheme, "{name}: resolved scheme");

        render_theme(&cx).unwrap_or_else(|error| panic!("{name}: render_theme failed: {error}"));
        let search = std::fs::read_to_string(&paths.markers.search_engine)
            .unwrap_or_else(|error| panic!("{name}: reading the search marker: {error}"));
        assert_eq!(
            search,
            field(expected, "search_engine"),
            "{name}: search-engine marker"
        );
        check_wallpaper(name, &cx, expected, &fixture_home, &home);
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
