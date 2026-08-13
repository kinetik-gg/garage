//! `response()` and `USAGE`: the envelope every settings-backend command prints, and the
//! help text `garage help` prints verbatim.
//!
//! `response()` is one line of compact JSON -- `{"ok", "data", "error"}` -- printed to
//! stdout by every settings command. The three legacy plumbing commands (`doctor`, `repair`,
//! `update`) always print lines; `reconcile` prints lines unless `--json` asks it to use this
//! same envelope. The watchdog runs unattended and has nobody to report to. `ok` is simply
//! `not error`, so a
//! caller never has to reconcile the two fields disagreeing; `data` is `null` for a command
//! that only signals success (`render`, `apply`, `action`) and the actual payload for one
//! that answers a question (`snapshot`, `set`, `display-test`, `theme-sync`). Separators are
//! trimmed to `(",", ":")` because this is read by a JSON parser on the other end, never by
//! a person, and the QML client's own stdout channel is not free.
//!
//! The object is written field by field rather than assembled and handed to a serialiser,
//! for one reason: `json.dumps` of a Python dict emits the keys in insertion order, and the
//! Python's insertion order is `ok`, `data`, `error`. Writing them out in that order costs
//! nothing and removes the question of whether some map type is preserving it.
//!
//! `USAGE` is the text `garage help`, `garage -h` and `garage --help` all print unchanged,
//! and the only place command names and their arguments are written down together for a
//! person to read. It is split into two groups matching the two kinds of command this binary
//! has: the human commands (`doctor`, `repair`, `update`, `reconcile`, `help`) and the
//! settings backend, which always prints exactly one JSON object. Reconcile is the documented
//! hybrid. `_display-watchdog` is deliberately absent from it -- it is the watchdog's
//! own re-entry point, not something a person types.

use serde_json::Value;

use crate::pyjson;

/// What `garage help` prints. The Python's `print(USAGE, end="")`, so the trailing newline
/// is the one this string already carries and nothing adds a second.
pub(crate) const USAGE: &str = r#"Usage: garage [COMMAND [ARGUMENTS]]

Human commands:
  doctor [--report]       check this install and report what is wrong (exit 1 if any)
                          --report prints the same checks as JSON, to paste into a bug
  repair [--reset]        recover an unparseable preferences.toml; reports unless
                          given --reset, which backs the file up and writes a fresh one
  update [--dry-run]      pull, sweep dead links, re-converge on the checkout, reload
  reconcile [--dry-run] [--prune] [--json]
                          converge manifest paths; optionally guarded-prune obsolete ones
  help                    print this

Settings backend (each prints one JSON object: {"ok","data","error"}):
  snapshot                the whole live state, and the default with no command
  render                  rewrite every generated fragment (files only, applies nothing)
  render-idle             rewrite hypridle.conf only (hypridle's ExecStartPre)
  render-bar              rewrite the bar's fragments only (waybar's ExecStartPre)
  render-wallpaper        rewrite hyprpaper.conf only (hyprpaper's ExecStartPre)
  apply                   render, then push everything into the running session
  set KEY JSON_VALUE      write one preference and apply just that change
  action NAME [JSON]      run a one-shot action (volume, dnd, keybind.*, defaults.*)
  display-test JSON       apply a display layout for 15s, pending confirmation
  display-confirm TOKEN   keep the layout under test
  display-revert TOKEN    put the previous layout back
  theme-sync              switch light/dark if the schedule says so (timer)
  night-shift-sync        re-evaluate the night shift window (timer)
"#;

/// `print(json.dumps({"ok": not error, "data": data, "error": error}, separators=(",", ":")))`.
///
/// One line, on stdout, and the newline `println!` adds is `print()`'s own.
pub(crate) fn emit(data: &Value, error: &str) {
    println!("{}", envelope(data, error));
}

/// The envelope as a string, which is what the tests compare and [`emit`] prints.
#[must_use]
pub(crate) fn envelope(data: &Value, error: &str) -> String {
    let mut out = String::from("{\"ok\":");
    out.push_str(if error.is_empty() { "true" } else { "false" });
    out.push_str(",\"data\":");
    pyjson::write_value(data, &mut out);
    out.push_str(",\"error\":");
    pyjson::write_string(error, &mut out);
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{envelope, USAGE};

    /// Every expectation here is the literal output of the Python's own `response()`, run
    /// with the same argument and pasted in. Not paraphrased: the point of the check is
    /// that the two byte strings are equal, and a hand-written approximation of one of them
    /// would only prove this file agrees with itself.
    #[test]
    fn a_command_that_only_signals_success_says_so_with_true() {
        assert_eq!(
            envelope(&Value::Bool(true), ""),
            r#"{"ok":true,"data":true,"error":""}"#
        );
    }

    #[test]
    fn a_command_with_nothing_to_say_still_carries_all_three_fields() {
        assert_eq!(
            envelope(&Value::Null, ""),
            r#"{"ok":true,"data":null,"error":""}"#
        );
    }

    #[test]
    fn an_unknown_command_is_ok_false_with_the_message_and_a_null_payload() {
        assert_eq!(
            envelope(&Value::Null, "Unknown command: definitely-not-a-command"),
            r#"{"ok":false,"data":null,"error":"Unknown command: definitely-not-a-command"}"#
        );
    }

    #[test]
    fn the_set_usage_refusal_travels_in_the_same_envelope() {
        assert_eq!(
            envelope(&Value::Null, "Usage: garage set KEY JSON_VALUE"),
            r#"{"ok":false,"data":null,"error":"Usage: garage set KEY JSON_VALUE"}"#
        );
    }

    #[test]
    fn a_payload_that_answers_a_question_keeps_its_own_shape() {
        assert_eq!(
            envelope(&json!({"token": "1f2e3d"}), ""),
            r#"{"ok":true,"data":{"token":"1f2e3d"},"error":""}"#
        );
        assert_eq!(
            envelope(&json!("dark"), ""),
            r#"{"ok":true,"data":"dark","error":""}"#
        );
        assert_eq!(
            envelope(&json!({"active": true, "applied": false}), ""),
            r#"{"ok":true,"data":{"active":true,"applied":false},"error":""}"#
        );
    }

    #[test]
    fn a_message_carrying_anything_but_printable_ascii_is_escaped_not_passed_through() {
        assert_eq!(
            envelope(
                &Value::Null,
                "\u{2728} caf\u{e9} \u{1f680} \t\"quo\\ted\" \u{1}"
            ),
            "{\"ok\":false,\"data\":null,\"error\":\"\\u2728 caf\\u00e9 \\ud83d\\ude80 \
             \\t\\\"quo\\\\ted\\\" \\u0001\"}"
        );
    }

    #[test]
    fn reconcile_json_is_one_envelope_carrying_the_plan_and_result() {
        let report = garage_reconcile::Report {
            dry_run: true,
            prune: false,
            checkout: "/checkout".to_owned(),
            home: "/home/test".to_owned(),
            desired: Vec::new(),
            units: Vec::new(),
            actual: garage_reconcile::ActualState::default(),
            plan: Vec::new(),
            refused: Vec::new(),
            applied: 0,
        };
        let payload = serde_json::to_value(report).expect("report serializes");
        let text = envelope(&payload, "");
        assert_eq!(text.lines().count(), 1);
        let decoded: Value = serde_json::from_str(&text).expect("envelope is JSON");
        assert_eq!(decoded.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(decoded.get("error"), Some(&Value::String(String::new())));
        let data = decoded.get("data").expect("data field");
        assert_eq!(data.get("dry_run"), Some(&Value::Bool(true)));
        assert!(data.get("plan").is_some_and(Value::is_array));
        assert_eq!(data.get("applied"), Some(&Value::from(0)));
    }

    /// The help text against the Python's own, extracted from the backend at the time this
    /// task landed and checked in beside this file. Byte-for-byte: `garage help` is a
    /// differential scenario in the active `cli` family, and this is the same claim made
    /// where a developer sees it fail first.
    #[test]
    fn the_help_text_is_the_pythons_verbatim() {
        assert_eq!(USAGE, include_str!("fixtures/usage.txt"));
        assert!(USAGE.ends_with("re-evaluate the night shift window (timer)\n"));
    }
}
