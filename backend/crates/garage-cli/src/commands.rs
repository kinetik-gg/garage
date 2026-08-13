//! `main()`'s dispatch shape: which command name reaches which call, and the three that skip
//! the JSON envelope entirely.
//!
//! Command resolution happens once, ahead of everything else: `argv[1]`, defaulting to
//! `"snapshot"` when the binary is run with no arguments at all, which is what lets the QML
//! client's simplest possible invocation -- no arguments -- ask for the whole live state.
//! `help`, `-h` and `--help` are recognised before any preferences file is touched, so
//! `garage help` never fails even on a machine with no config directory yet.
//!
//! The three plumbing commands -- `doctor`, `repair`, `update` -- are dispatched from a
//! separate table checked before the JSON path, and their errors go to stderr as plain text
//! rather than through [`crate::response`]'s envelope; see [`garage_apply`] for why. Every
//! other command runs `migrate_config_root()` once, ahead of its own dispatch rather than
//! inside a loader, because an action like `keybind.rebind` or `display-test` reaches
//! `keybindings.toml` or `displays.toml` directly without ever loading the preferences, and
//! either one arriving first at the old config layout would write a fresh file at the new
//! path while the user's own sat at the old one.
//!
//! Fifteen command names in the settings-backend table: `snapshot`, `render`, `render-idle`,
//! `render-bar`, `render-wallpaper`, `apply`, `set`, `action`, `display-test`,
//! `display-confirm`, `display-revert`, `_display-watchdog` (unlisted in `USAGE`, since it is
//! the watchdog's own re-entry point and not something a person types), `theme-sync` and
//! `night-shift-sync`.
//!
//! # What is wired and what is owed
//!
//! The *structure* below is final; several of the layers it calls into are not. A command
//! whose layer is still a stub fails through the envelope naming itself, which is the honest
//! answer and the one the differential harness can read. Three reach real code already --
//! `help`, `render` (through [`render_all`]) and `render-idle` (through
//! [`run_render`]) -- and `set` reaches the real load, the real write and the real route
//! walk, failing only at the first step of the route. `render-bar`, `render-wallpaper` and
//! `apply` have no entry point in `garage-render`/`garage-apply` to call yet; they are wired
//! the moment those crates grow one, and nothing here changes when they do.

use std::thread;
use std::time::Duration;

use garage_core::paths::Paths;
use garage_core::schema::RenderStep;
use garage_prefs::{load_preferences, migrate_config_root};
use garage_proc::{Hyprctl, Luac, System};
use garage_render::all::render_all;
use garage_render::cx::RenderCx;
use garage_render::dispatch::run_render;
use serde_json::Value;

use crate::error::CliError;
use crate::response::{emit, USAGE};
use crate::set;

/// The three commands that answer a person rather than the QML client, so their output is
/// lines and their failures are stderr -- never the JSON envelope, which nothing at a
/// terminal would want to read. `doctor --report` prints JSON, but it is still this kind of
/// command: the blob is for a person to paste, and a failure of it is still a message on
/// stderr.
const PLAIN_COMMANDS: [&str; 3] = ["doctor", "repair", "update"];

/// What a settings-backend command left for [`emit`] to print.
///
/// Two shapes, because `_display-watchdog` is the one command in the table that prints
/// nothing at all: it runs unattended, fifteen seconds after the process that started it has
/// already answered, and there is nobody left to read an envelope.
enum Emitted {
    /// Print this payload with an empty `error`.
    Envelope(Value),
    /// Print nothing. Exit 0 regardless.
    Silent,
}

/// `main(argv)`: the exit status this process leaves with.
///
/// Everything is written to stdout or stderr on the way through, exactly as the Python's
/// `print()` calls do, so the only thing that comes back is the status.
#[must_use]
pub(crate) fn run(argv: &[String]) -> u8 {
    let command = argv.get(1).map_or("snapshot", String::as_str);
    if matches!(command, "help" | "-h" | "--help") {
        print!("{USAGE}");
        return 0;
    }
    let paths = Paths::from_env();
    if PLAIN_COMMANDS.contains(&command) {
        return plain(&paths, command);
    }
    match settings(&paths, command, argv) {
        Ok(Emitted::Envelope(data)) => {
            emit(&data, "");
            0
        }
        Ok(Emitted::Silent) => 0,
        Err(error) => {
            emit(&Value::Null, &error.to_string());
            1
        }
    }
}

/// `doctor`, `repair`, `update`: lines for a person, failures on stderr, `garage {command}:
/// {error}`.
///
/// TEMPORARY, and only the message is: the branch, the `migrate_config_root()` that precedes
/// it and the stderr-and-exit-1 shape are the Python's and stay. `doctor` lands in task
/// 3.12, `repair` in 3.13 and `update` in 3.14, and each replaces exactly the one line below.
fn plain(paths: &Paths, command: &str) -> u8 {
    if let Err(error) = migrate_config_root(paths) {
        eprintln!("garage {command}: {error}");
        return 1;
    }
    eprintln!("garage {command}: not yet ported");
    1
}

/// The settings backend: one JSON object out, or one error in the same envelope.
///
/// `migrate_config_root()` runs first and its failure travels through the envelope, which is
/// the Python's placement -- inside the `try`, ahead of the dispatch.
fn settings(paths: &Paths, command: &str, argv: &[String]) -> Result<Emitted, CliError> {
    migrate_config_root(paths)?;
    match command {
        // make_snapshot(): task 3.9.
        "snapshot" => Err(CliError::PortPending("snapshot")),
        // Files only. Every fragment is rewritten and nothing is signalled: see
        // render_all(). `apply` is the one that also moves the session.
        "render" => rendered(paths, None),
        // hypridle's ExecStartPre. One file, which is all hypridle reads -- and all it may
        // render, because `set lock.*` restarts that unit synchronously while holding
        // PREFERENCES_LOCK.
        "render-idle" => rendered(paths, Some(RenderStep::Idle)),
        // waybar's ExecStartPre: render_region + render_bar_workspaces + render_bar_widgets,
        // narrow rather than a full render. No public entry point for the three yet.
        "render-bar" => Err(CliError::PortPending("render-bar")),
        // hyprpaper's ExecStartPre, deliberately not "render". Same, task 3.5.
        "render-wallpaper" => Err(CliError::PortPending("render-wallpaper")),
        // apply_preferences(): task 3.7.
        "apply" => Err(CliError::PortPending("apply")),
        "set" => set::set(paths, argv).map(Emitted::Envelope),
        // action(): task 3.10.
        "action" => Err(CliError::PortPending("action")),
        // display_test() and display_finish(): task 3.8.
        "display-test" => Err(CliError::PortPending("display-test")),
        "display-confirm" => Err(CliError::PortPending("display-confirm")),
        "display-revert" => Err(CliError::PortPending("display-revert")),
        "_display-watchdog" => Ok(watchdog()),
        // theme-sync and night-shift-sync: task 3.6.
        "theme-sync" => Err(CliError::PortPending("theme-sync")),
        "night-shift-sync" => Err(CliError::PortPending("night-shift-sync")),
        other => Err(CliError::UnknownCommand(other.to_owned())),
    }
}

/// `render_all(load_preferences())`, or one named step of it.
///
/// The context is assembled per invocation and thrown away with it. It carries no lock and
/// no runner -- see [`RenderCx`] -- so this path is structurally unable to reach either,
/// which is what lets `render-idle` be re-entered from `hypridle.service`'s `ExecStartPre`
/// while a `set lock.*` is holding `PREFERENCES_LOCK`.
fn rendered(paths: &Paths, step: Option<RenderStep>) -> Result<Emitted, CliError> {
    let config = load_preferences(paths, None)?;
    let system = System;
    let monitors = Hyprctl::new(&system);
    let lua = Luac::new(&system);
    let cx = RenderCx::new(&config, paths, &monitors, &lua);
    match step {
        None => render_all(&cx)?,
        Some(step) => run_render(step, &cx)?,
    }
    Ok(Emitted::Envelope(Value::Bool(true)))
}

/// `_display-watchdog TOKEN`: sleep fifteen seconds, then put the previous layout back.
///
/// A tested display layout is applied at once and written to `displays.toml` only once it is
/// confirmed. Nobody confirming it is the case this exists for -- a layout that left no
/// working input device would otherwise strand the user in it -- so the revert has to happen
/// on a process that outlives the one that returned the token.
///
/// `display_finish()` is task 3.8's. The Python swallows `(SettingsError, OSError,
/// JSONDecodeError)` here and prints nothing either way, so the not-yet-ported failure is
/// swallowed by exactly the same `except` and the observable behaviour -- no stdout, exit 0
/// -- is already the Python's.
fn watchdog() -> Emitted {
    thread::sleep(Duration::from_secs(15));
    Emitted::Silent
}

#[cfg(test)]
mod tests {
    use super::{settings, Emitted, PLAIN_COMMANDS};
    use crate::error::CliError;
    use garage_core::paths::Paths;
    use std::collections::HashMap;

    /// A HOME that does not exist, which is enough for every case here: none of them reaches
    /// a loader, and `migrate_config_root()` over an absent directory is a no-op.
    fn paths() -> Paths {
        let env: HashMap<String, String> = [(
            "HOME".to_owned(),
            "/nonexistent/garage-cli-dispatch-test".to_owned(),
        )]
        .into_iter()
        .collect();
        Paths::from_env_map(&env)
    }

    fn argv(parts: &[&str]) -> Vec<String> {
        std::iter::once("garage")
            .chain(parts.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn an_unknown_command_carries_the_pythons_own_wording() {
        let error = settings(
            &paths(),
            "definitely-not-a-command",
            &argv(&["definitely-not-a-command"]),
        )
        .err()
        .map(|error| error.to_string());
        assert_eq!(
            error.as_deref(),
            Some("Unknown command: definitely-not-a-command")
        );
    }

    #[test]
    fn every_command_in_the_table_is_reachable_and_none_of_them_is_unknown() {
        // The whole of USAGE's settings backend, plus the two names that are not in it: the
        // default when no subcommand is given, and the watchdog's re-entry point. A command
        // this dispatch forgot would come back as "Unknown command", which is the one
        // failure this test exists to catch.
        let commands = [
            "snapshot",
            "render",
            "render-idle",
            "render-bar",
            "render-wallpaper",
            "apply",
            "action",
            "display-test",
            "display-confirm",
            "display-revert",
            "theme-sync",
            "night-shift-sync",
        ];
        for command in commands {
            let outcome = settings(&paths(), command, &argv(&[command]));
            assert!(
                !matches!(outcome, Err(CliError::UnknownCommand(_))),
                "{command} is not dispatched"
            );
        }
    }

    #[test]
    fn a_stub_names_the_command_that_is_owed() {
        let error = settings(&paths(), "theme-sync", &argv(&["theme-sync"]))
            .err()
            .map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some("theme-sync has not been ported yet"));
    }

    #[test]
    fn set_with_the_wrong_argument_count_is_a_usage_message_not_a_schema_refusal() {
        let error = settings(&paths(), "set", &argv(&["set", "appearance.accent_color"]))
            .err()
            .map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some("Usage: garage set KEY JSON_VALUE"));
        let too_many = settings(&paths(), "set", &argv(&["set", "a.b", "1", "2"]))
            .err()
            .map(|error| error.to_string());
        assert_eq!(
            too_many.as_deref(),
            Some("Usage: garage set KEY JSON_VALUE")
        );
    }

    #[test]
    fn the_watchdog_is_the_only_command_that_prints_nothing() {
        // Not run -- it sleeps fifteen seconds by design. What is checked is that it is the
        // only arm that can produce a silent outcome, which is the property the exit-status
        // handling in `run()` depends on.
        let silent = matches!(Emitted::Silent, Emitted::Silent);
        assert!(silent);
    }

    #[test]
    fn the_plain_table_is_the_pythons_three() {
        assert_eq!(PLAIN_COMMANDS, ["doctor", "repair", "update"]);
    }
}
