//! `run()` and `json_command()`: the two shapes every call out of this crate takes.
//!
//! The Python's `run()` (garage:1462-1468) is one function with two modes, and the difference
//! between them is the whole of its error handling. Unchecked, a command that could not be
//! run at all -- no such binary, a timeout -- comes back as the synthesised
//! `CompletedProcess(command, 1, "", str(error))`, so a caller sees one shape rather than two
//! and may read `returncode` without a `try` around it. Checked, the same failure and a
//! non-zero exit both raise `SettingsError(str(error))`, where the text is Python's own --
//! `CalledProcessError.__str__`, which spells the argv with `repr()` -- and reaches the JSON
//! envelope verbatim. [`run`] and [`run_checked`] are those two modes.
//!
//! `json_command()` (garage:4946-4952) folds four different failures -- a missing binary, a
//! timeout, a non-zero exit, unparseable output -- into one fallback value, and every
//! `hyprctl -j` and `pactl -f json` reader in the file goes through it. `garage-proc`'s
//! `Hyprctl` deliberately does *not*: it keeps those failures as an `Err` so a caller may
//! choose, which is right for the one question a render asks and wrong for these readers,
//! which want the conflation exactly.
//!
//! Two entry points rather than one, because the Python's `fallback` argument is only ever
//! `[]` or `{}` and the two are read differently at every call site: [`json_list`] for the
//! readers that iterate (`hyprctl monitors`, `pactl list sinks`), [`json_object`] for the
//! readers that index (`hyprctl devices`, `pactl info`). A document of the wrong kind is the
//! fallback, which is what the Python's own `isinstance()` guard immediately after each call
//! amounts to.
//!
//! This module is the merge the workspace layer's `json_list()` asked for in as many words:
//! it lived in `crate::workspaces` until `display_snapshot()` and the audio, input and
//! date/time snapshots wanted the same conflation, and there was never anything in it
//! specific to workspaces.

use std::time::Duration;

use garage_core::pyrepr::{py_repr, PyValue};
use garage_core::traits::{Output, DEFAULT_RUN_TIMEOUT};
use serde_json::{Map, Value};

use crate::cx::SessionCx;
use crate::error::ApplyError;

/// `run(command)` with `check=False`: never fails, and a command that could not be run at all
/// comes back as the Python's synthesised `CompletedProcess(command, 1, "", str(error))`.
pub(crate) fn run(cx: &SessionCx<'_>, command: &[&str]) -> Output {
    run_within(cx, command, DEFAULT_RUN_TIMEOUT)
}

/// [`run`] with the Python's `timeout=` given explicitly -- only `solid_wallpaper()`'s
/// `magick` run needs it, at fifteen seconds rather than four.
pub(crate) fn run_within(cx: &SessionCx<'_>, command: &[&str], timeout: Duration) -> Output {
    cx.proc()
        .run(command, timeout)
        .unwrap_or_else(|error| Output {
            status: 1,
            stdout: String::new(),
            stderr: error.detail,
        })
}

/// `run(command, check=True)`: a non-zero exit is a refusal, in Python's own words.
///
/// `subprocess.run(check=True)` raises `CalledProcessError`, which the Python's `run()`
/// catches as a `SubprocessError` and re-raises as `SettingsError(str(error))` -- so what
/// reaches the envelope is `CalledProcessError.__str__`: `Command '{cmd!r}' returned non-zero
/// exit status {code}.`, with the argv spelled as a Python list repr. Reproduced here rather
/// than replaced with a message of this port's own, because that string is what the pane
/// shows a user when `wpctl` or `loginctl` refuses.
///
/// # Errors
///
/// [`ApplyError::Settings`] carrying that text, or -- for a command that could not be started
/// at all -- whatever the runner reported, which is the `str(error)` of the `OSError` the
/// Python's own `except` re-raises.
pub(crate) fn run_checked(cx: &SessionCx<'_>, command: &[&str]) -> Result<Output, ApplyError> {
    let result = cx
        .proc()
        .run(command, DEFAULT_RUN_TIMEOUT)
        .map_err(|error| ApplyError::Settings(error.detail))?;
    if result.status == 0 {
        return Ok(result);
    }
    Err(ApplyError::Settings(format!(
        "Command '{}' returned non-zero exit status {}.",
        argv_repr(command),
        result.status
    )))
}

/// `repr(list_of_str)`: `['wpctl', 'set-volume', '@DEFAULT_AUDIO_SINK@', '0.5']`.
fn argv_repr(command: &[&str]) -> String {
    let parts: Vec<String> = command
        .iter()
        .map(|word| py_repr(PyValue::Str(word)))
        .collect();
    format!("[{}]", parts.join(", "))
}

/// `json_command(command, [])`: the parsed array, and an empty one for every way of not
/// getting one. See the module doc for why this conflation is wanted here.
pub(crate) fn json_list(cx: &SessionCx<'_>, command: &[&str]) -> Vec<Value> {
    match parsed(cx, command) {
        Some(Value::Array(items)) => items,
        // Anything else is either falsy (`or []`) or iterates into values no caller's
        // `isinstance(..., dict)` guard accepts, which is the same empty answer.
        _ => Vec::new(),
    }
}

/// `json_command(command, {})`: the parsed object, and an empty one otherwise.
pub(crate) fn json_object(cx: &SessionCx<'_>, command: &[&str]) -> Map<String, Value> {
    match parsed(cx, command) {
        Some(Value::Object(map)) => map,
        // The Python guards each of these call sites with `isinstance(..., dict)` and answers
        // the fallback when it fails, which is this empty map.
        _ => Map::new(),
    }
}

/// The shared half: run it, and parse stdout only if the command actually succeeded.
fn parsed(cx: &SessionCx<'_>, command: &[&str]) -> Option<Value> {
    let result = run(cx, command);
    if result.status != 0 {
        return None;
    }
    serde_json::from_str::<Value>(&result.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::{argv_repr, json_list, json_object, run, run_checked};
    use crate::testing::{Script, World};

    #[test]
    fn an_unrunnable_command_comes_back_as_the_pythons_synthesised_failure() {
        let world = World::plain(
            "run-refused",
            Script::new().refusing("hyprctl reload", "no such file"),
        );
        world.with(|cx| {
            let result = run(cx, &["hyprctl", "reload"]);
            assert_eq!(result.status, 1);
            assert_eq!(result.stdout, "");
            assert_eq!(result.stderr, "no such file");
        });
    }

    #[test]
    fn checked_spells_its_refusal_the_way_python_spells_it() {
        let world = World::plain(
            "run-checked",
            Script::new().answering("wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.5", 3, "", ""),
        );
        world.with(|cx| {
            let error = run_checked(cx, &["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", "0.5"])
                .expect_err("a non-zero exit is a refusal");
            assert_eq!(
                error.to_string(),
                "Command '['wpctl', 'set-volume', '@DEFAULT_AUDIO_SINK@', '0.5']' \
                 returned non-zero exit status 3."
            );
        });
    }

    #[test]
    fn a_list_reader_and_an_object_reader_each_fall_back_to_their_own_shape() {
        let world = World::plain(
            "json-shapes",
            Script::new()
                .answering("hyprctl monitors -j", 0, "[{\"name\": \"eDP-1\"}]", "")
                .answering("hyprctl devices -j", 0, "not json at all", ""),
        );
        world.with(|cx| {
            assert_eq!(json_list(cx, &["hyprctl", "monitors", "-j"]).len(), 1);
            assert!(json_object(cx, &["hyprctl", "devices", "-j"]).is_empty());
            // A document of the wrong kind is the fallback, which is what the Python's
            // `isinstance()` guard after each call amounts to.
            assert!(json_list(cx, &["hyprctl", "devices", "-j"]).is_empty());
        });
    }

    #[test]
    fn the_argv_repr_is_pythons_list_repr() {
        assert_eq!(argv_repr(&["a", "b c"]), "['a', 'b c']");
        assert_eq!(argv_repr(&[]), "[]");
    }
}
