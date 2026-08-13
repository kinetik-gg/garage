//! The display transaction's four commands: `display-test`, `display-confirm`,
//! `display-revert` and the watchdog's own re-entry point.
//!
//! Split out of [`crate::commands`] because the watchdog needs two things no other arm does
//! -- a session context and the `setsid()` that turns a detached child into a detached
//! *session* -- and because these four are the one group in the table that talks to
//! `displays.toml` rather than to `preferences.toml`.
//!
//! All four assemble a [`SessionCx`] per invocation and throw it away with it. The
//! preferences are loaded only because a context contains a `RenderCx` and a `RenderCx`
//! contains one; nothing on this path reads a preference, exactly as the Python's
//! `display_test()` and `display_finish()` read none.

use std::thread;
use std::time::Duration;

use garage_apply::displays::transaction::{self, CONFIRM_WINDOW};
use garage_apply::SessionCx;
use garage_core::paths::Paths;
use garage_core::traits::Runner;
use garage_prefs::load_preferences;
use garage_proc::run::enter_new_session;
use garage_proc::{Hyprctl, Luac};
use garage_render::cx::RenderCx;
use serde_json::Value;

use crate::error::CliError;

/// `display-test JSON`: `response({"token": display_test(json.loads(argv[2]))})`.
///
/// The preferences are loaded only to build the context -- nothing on this path reads one,
/// exactly as the Python's `display_test()` reads none. `HYPR_PRIMARY_MONITOR` is read here,
/// at the outermost entry point, so every function below it takes the value as an argument
/// and a test can name it without touching the process environment.
///
/// **Deviation, stated plainly:** `garage display-test` with no argument raises `IndexError`
/// in the Python -- uncaught, so a traceback on stderr and exit 1 -- because `argv[2]` is
/// indexed rather than checked. Here it is the envelope's own refusal, in the shape `set`'s
/// argument-count guard already uses. The exit status is 1 either way.
pub(crate) fn display_test(
    paths: &Paths,
    proc: &dyn Runner,
    argv: &[String],
) -> Result<Value, CliError> {
    let payload: Value = serde_json::from_str(
        argv.get(2)
            .ok_or(CliError::DisplayUsage("garage display-test JSON"))?,
    )?;
    let config = load_preferences(paths, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let cx = SessionCx::new(RenderCx::new(&config, paths, &monitors, &lua), proc);
    let token = transaction::display_test(&cx, &payload, &primary_from_environment())?;
    Ok(serde_json::json!({ "token": token }))
}

/// `display-confirm TOKEN` and `display-revert TOKEN`: `display_finish(argv[2], confirm)`
/// followed by `response(True)`. Same missing-argument deviation as [`display_test`].
pub(crate) fn display_finish(
    paths: &Paths,
    proc: &dyn Runner,
    argv: &[String],
    confirm: bool,
) -> Result<Value, CliError> {
    let usage = if confirm {
        "garage display-confirm TOKEN"
    } else {
        "garage display-revert TOKEN"
    };
    let token = argv.get(2).ok_or(CliError::DisplayUsage(usage))?;
    let config = load_preferences(paths, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let cx = SessionCx::new(RenderCx::new(&config, paths, &monitors, &lua), proc);
    transaction::display_finish(&cx, token, confirm)?;
    Ok(Value::Bool(true))
}

/// `_display-watchdog TOKEN`: sleep fifteen seconds, then put the previous layout back.
///
/// A tested display layout is applied at once and written to `displays.toml` only once it is
/// confirmed. Nobody confirming it is the case this exists for -- a layout that left no
/// working input device would otherwise strand the user in it -- so the revert has to happen
/// on a process that outlives the one that returned the token.
///
/// [`enter_new_session`] runs first, before the sleep: this process was started by
/// [`Runner::spawn_self_detaching`](garage_core::traits::Runner::spawn_self_detaching), which
/// deliberately left it in its parent's process group so that `setsid()` here would be
/// permitted. Together the two are `start_new_session=True` -- and this half is what makes
/// the watchdog survive the terminal that started the `display-test` being closed inside the
/// fifteen seconds. See `garage_proc::run::enter_new_session` for the residual window.
///
/// Every failure is swallowed and nothing is printed, which is the Python's own
/// `except (SettingsError, OSError, json.JSONDecodeError): pass` around this call: there is
/// nobody left to read an envelope, and exit 0 regardless.
pub(crate) fn watchdog(paths: &Paths, proc: &dyn Runner, argv: &[String]) {
    enter_new_session();
    thread::sleep(Duration::from_secs(CONFIRM_WINDOW));
    let Some(token) = argv.get(2) else {
        return;
    };
    if let Ok(config) = load_preferences(paths, None) {
        let monitors = Hyprctl::new(proc);
        let lua = Luac::new(proc);
        let cx = SessionCx::new(RenderCx::new(&config, paths, &monitors, &lua), proc);
        drop(transaction::display_finish(&cx, token, false));
    }
}

/// `os.environ.get("HYPR_PRIMARY_MONITOR", "")`, read at call time.
fn primary_from_environment() -> String {
    std::env::var("HYPR_PRIMARY_MONITOR").unwrap_or_default()
}
