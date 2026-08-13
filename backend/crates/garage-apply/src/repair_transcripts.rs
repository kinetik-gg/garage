//! `tests/test_recovery.py`'s `Repair` class, replayed byte for byte.
//!
//! `testdata/repair_transcripts.json` is the output of a throwaway generator (not
//! committed -- the same arrangement `doctor::parity` uses) which drives the Python's own
//! `repair()` with `BACKUP_STAMP` pinned to `"fixed"`, exactly as that test monkeypatches
//! it: the name carries a whole-second stamp, so two repairs in the same second collide,
//! which is the case worth asserting and the one a test cannot schedule.
//!
//! Compared: the transcript, the exit status, the refusal message for a bad argument, and
//! the whole of `~/.config/garage` afterwards -- which is what proves the other three user
//! files were left alone and that the first run's backup survived the second.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::repair::repair_at;
use crate::repair::tests::scratch;

const TRANSCRIPTS: &str = include_str!("../testdata/repair_transcripts.json");

/// The one line whose text is `tomllib`'s rather than the `toml` crate's, elided
/// on both sides.
///
/// A known and accepted parity gap, the same one
/// [`garage_prefs::PrefsError::Unreadable`] documents: the shape is the Python's --
/// the state, a colon, the parser's complaint -- but the complaint itself is `toml`'s
/// rather than `tomllib`'s, and the two spell a syntax error differently. `toml`'s
/// also spans several lines, because it prints the offending line with a caret under
/// it, so everything up to the `size` line goes with it. Nothing downstream matches on
/// any of it: it is printed, once, for a person holding a broken file, and it is the
/// same text `garage doctor` prints for the same file.
///
/// The `modified` line is the other replacement and is not a gap at all -- it is the
/// file's own mtime, which is the moment the fixture planted it.
fn elide(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut inside_parser_complaint = false;
    for line in text.lines() {
        // `  size ` is the line the complaint runs up to, whether the complaint was one
        // line (`tomllib`) or four (`toml`, which prints a caret under the offence).
        if inside_parser_complaint && !line.starts_with("  size ") {
            continue;
        }
        inside_parser_complaint = false;
        if let Some((head, _)) = line.split_once("does NOT parse: ") {
            lines.push(format!("{head}does NOT parse: <PARSER>"));
            inside_parser_complaint = true;
        } else if line.starts_with("  modified  ") {
            lines.push("  modified  <MTIME>".to_owned());
        } else {
            lines.push(line.to_owned());
        }
    }
    lines.join("\n") + "\n"
}

#[test]
fn every_scenario_prints_and_leaves_behind_what_the_python_did() {
    let document: Value =
        serde_json::from_str(TRANSCRIPTS).expect("testdata/repair_transcripts.json is valid JSON");
    let scenarios = document
        .get("scenarios")
        .and_then(Value::as_array)
        .expect("a list of scenarios");
    assert!(scenarios.len() >= 7, "the corpus lost scenarios");
    for scenario in scenarios {
        check(scenario);
    }
}

/// One scenario: plant the world, run every one of its `repair` invocations in order, and
/// compare what is left behind.
fn check(scenario: &Value) {
    let name = scenario["name"].as_str().unwrap_or("unnamed");
    let (home, paths) = scratch(name);
    if scenario["has_initial"].as_bool().unwrap_or(false) {
        std::fs::write(
            &paths.host.preferences,
            scenario["initial"].as_str().unwrap_or(""),
        )
        .expect("plant preferences.toml");
    }
    plant_neighbours(&paths, scenario);
    link_defaults(&paths);
    for run in scenario["runs"].as_array().expect("a list of runs") {
        replay(&paths, name, run);
    }
    let expected: BTreeMap<String, String> = scenario["tree_after"]
        .as_object()
        .expect("tree_after")
        .iter()
        .map(|(file, text)| (file.clone(), text.as_str().unwrap_or("").to_owned()))
        .collect();
    assert_eq!(
        config_tree(&paths),
        expected,
        "{name}: what was left on disk"
    );
    drop(std::fs::remove_dir_all(&home));
}

/// One `repair` invocation: the transcript, and either the exit status or the refusal.
fn replay(paths: &garage_core::paths::Paths, name: &str, run: &Value) {
    let argv: Vec<String> = run["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let mut out = String::new();
    let outcome = repair_at(&mut out, paths, &argv, "fixed");
    assert_eq!(
        elide(&out),
        elide(run["stdout"].as_str().unwrap_or("")),
        "{name}: the transcript of {argv:?}"
    );
    match run["status"].as_i64() {
        Some(status) => assert_eq!(
            i64::from(outcome.expect("the run succeeded")),
            status,
            "{name}: the exit status of {argv:?}"
        ),
        // `status: null` is the Python raising SettingsError, which main() turns into
        // `garage repair: {error}` on stderr and exit 1.
        None => assert_eq!(
            outcome.err().map(|error| error.to_string()).as_deref(),
            run["error"].as_str(),
            "{name}: the refusal for {argv:?}"
        ),
    }
}

/// The other three user files, planted so "only preferences.toml is touched" has
/// something to be true about.
fn plant_neighbours(paths: &garage_core::paths::Paths, scenario: &Value) {
    let Some(neighbours) = scenario["neighbours"].as_object() else {
        return;
    };
    for (file, text) in neighbours {
        std::fs::write(paths.root.join(file), text.as_str().unwrap_or(""))
            .expect("plant a neighbour");
    }
}

/// Every real file under `~/.config/garage`, which is what the Python's own `tree()`
/// helper collects: the symlinked shipped defaults are skipped on both sides.
fn config_tree(paths: &garage_core::paths::Paths) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for entry in std::fs::read_dir(&paths.root).expect("read the config dir") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        if !path.is_symlink() && path.is_file() {
            found.insert(
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(&path).unwrap_or_default(),
            );
        }
    }
    found
}

/// The shipped defaults as a stow symlink, so the confirming load has a layer 1.
fn link_defaults(paths: &garage_core::paths::Paths) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../desktop/.config/garage/preferences.defaults.toml");
    if !paths.defaults_path.exists() {
        std::os::unix::fs::symlink(source, &paths.defaults_path).expect("defaults symlink");
    }
}
