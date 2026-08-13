//! Byte-parity tests for `keybind_action()` itself: one call per fixture entry, from the
//! files it started with to the files it left.
//!
//! Split from [`super::parity`], which carries the fixture loader and the other five
//! families, only because the two together would be one file past this repo's 500-line
//! shape rule. Everything shared -- the fixture JSON, `same_outcome`, `mask_ids`, the
//! scratch `HOME` -- is that module's.
//!
//! The action family is the end-to-end one. Each entry lays a published catalog and a stored
//! `keybindings.toml` down in a scratch `HOME`, runs one operation against a compositor that
//! reloads without complaint, and compares both files it may have written, byte for byte,
//! against what the Python left in the same situation -- including the cases where the change
//! was refused and neither file may have moved at all.

use std::path::Path;
use std::time::Duration;

use garage_core::schema::defaults::Defaults;
use garage_core::traits::{Output, RunError, Runner};
use garage_render::cx::RenderCx;
use serde_json::Value;

use crate::cx::SessionCx;
use crate::keybind::parity::{
    field, fixtures, lay_out, mask_ids, same_outcome, scratch_paths, text, LuaAccepts, NoMonitors,
};
use crate::keybind::{keybind_action, KeybindRequest};

/// A compositor that reloads without complaint and reports whichever bind set the fixture
/// says it had. `hyprctl` itself is never run: the Python side recorded these answers with
/// `run()` and `json_command()` replaced for exactly the same reason.
struct Compositor {
    binds: &'static str,
}

impl Runner for Compositor {
    fn run(&self, command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
        let stdout = if command.contains(&"binds") {
            self.binds
        } else {
            ""
        };
        Ok(Output {
            status: 0,
            stdout: stdout.to_owned(),
            stderr: String::new(),
        })
    }

    fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
        Ok(())
    }

    fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
        Ok(0)
    }
}

#[test]
fn every_action_leaves_the_files_the_python_left() {
    let all = fixtures();
    let cases = field(&all, "actions")
        .as_object()
        .expect("actions is an object");
    assert!(cases.len() >= 30, "the action table should not have shrunk");
    let defaults = Defaults::compiled().expect("the shipped defaults parse");
    for (name, case) in cases {
        one_action(name, case, &defaults);
    }
}

/// One `keybind_action()` call, from the files it started with to the files it left.
fn one_action(name: &str, case: &Value, defaults: &Defaults) {
    let paths = scratch_paths("action");
    let before = text(case, "before");
    lay_out(&paths, text(case, "catalog"), before);
    let monitors = NoMonitors;
    let lua = LuaAccepts;
    let proc = Compositor {
        binds: if text(case, "binds") == "none" {
            "[]"
        } else {
            "[{\"id\": 1}]"
        },
    };
    let cx = SessionCx::new(
        RenderCx::new(defaults.values(), &paths, &monitors, &lua),
        &proc,
    );
    let payload = field(case, "payload");
    let request = KeybindRequest {
        id: payload.get("id").and_then(Value::as_str),
        keys: payload.get("keys").and_then(Value::as_str),
        description: payload.get("description").and_then(Value::as_str),
        command: payload.get("command").and_then(Value::as_str),
    };
    let outcome = keybind_action(&cx, text(case, "operation"), &request);
    same_outcome(
        field(case, "outcome"),
        outcome.map(|()| Value::Null),
        |_| Value::Null,
        name,
    );
    same_file(
        &paths.host.keybindings,
        field(case, "after_toml"),
        before,
        name,
    );
    same_file(
        &paths.fragments.keybinds_data,
        field(case, "after_json"),
        before,
        name,
    );
    drop(std::fs::remove_dir_all(&paths.home));
}

/// A file's bytes against the fixture's, where JSON `null` means the Python left no file.
fn same_file(path: &Path, expected: &Value, source: &str, name: &str) {
    let held = std::fs::read_to_string(path).ok();
    match expected.as_str() {
        // An invented id is twelve hex characters neither side can predict, so a line
        // carrying one is compared with that field masked -- see `mask_ids`.
        Some(wanted) => assert_eq!(
            mask_ids(&held.unwrap_or_default(), source),
            mask_ids(wanted, source),
            "{name}: {}",
            path.display()
        ),
        None => assert_eq!(held, None, "{name}: {} should not exist", path.display()),
    }
}
