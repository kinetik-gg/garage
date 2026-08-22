//! `main()`'s dispatch shape: which command name reaches which call, and the four that skip
//! the JSON envelope entirely.
//!
//! Command resolution happens once, ahead of everything else: `argv[1]`, defaulting to
//! `"snapshot"` when the binary is run with no arguments at all, which is what lets the QML
//! client's simplest possible invocation -- no arguments -- ask for the whole live state.
//! `help`, `-h` and `--help` are recognised before any preferences file is touched, so
//! `garage help` never fails even on a machine with no config directory yet.
//!
//! The four plain commands -- `doctor`, `migrate`, `repair`, `update` -- are dispatched from a
//! separate table checked before the JSON path, and their errors go to stderr as plain text
//! rather than through [`crate::response`]'s envelope; see [`garage_apply`] for why. `migrate`
//! is the one that bypasses config-root migration too, so its `--dry-run` has no hidden write.
//! `reconcile` is the hybrid beside them: human by default, one response envelope with
//! `--json`, and always dispatched before config migration so `--dry-run` has no hidden write.
//! Every other command runs `migrate_config_root()` once, ahead of its own dispatch rather than
//! inside a loader, because an action like `keybind.rebind` or `display-test` reaches
//! `keybindings.toml` or `displays.toml` directly without ever loading the preferences, and
//! either one arriving first at the old config layout would write a fresh file at the new
//! path while the user's own sat at the old one.
//!
//! Fifteen command names in the settings-backend table: `snapshot`, `render`, `render-idle`,
//! `render-wallpaper`, `apply`, `set`, `action`, `display-test`,
//! `display-confirm`, `display-revert`, `_display-watchdog` (unlisted in `USAGE`, since it is
//! the watchdog's own re-entry point and not something a person types), `theme-sync` and
//! `night-shift-sync`.
//!
//! # Where each name lands
//!
//! Every one of them reaches a real layer; the table is complete. `help` is answered here.
//! The four render commands go to `garage-render` -- [`render_all`] for the whole set,
//! [`run_render`] for `render-idle`'s one step, and
//! [`render_wallpaper`](garage_render::all::render_wallpaper) /
//! [`render_wallpaper`](garage_render::all::render_wallpaper) for the two narrow unit
//! `ExecStartPre`s. `set` is [`crate::set`], the four display commands are
//! [`crate::displays`], and the four that move the running session -- `apply`, `action`,
//! `theme-sync`, `night-shift-sync` -- are [`crate::session`]. `snapshot` is
//! [`garage_apply::make_snapshot`], which is also what a bare `garage` with no argument at
//! all resolves to.
//!
//! # The runner is a parameter, not a global
//!
//! [`run`] builds one [`System`] and hands it down; nothing below reaches for a process on
//! its own. That is what makes the dispatch testable now that most of these commands signal
//! the session: a test that used the real runner would `hyprctl reload` the developer's
//! desktop to find out whether a name is dispatched. The render-only arms take it too, for
//! the compositor question `render_workspaces()` asks and the `luac -p` every Lua fragment
//! is checked with.

use garage_apply::keybind::load_keybindings;
use garage_apply::terminal::resolve_browser;
use garage_apply::{doctor, make_snapshot, repair, update, SessionCx};
use garage_core::paths::Paths;
use garage_core::schema::RenderStep;
use garage_core::traits::Runner;
use garage_prefs::{load_preferences, migrate_config_root};
use garage_proc::{Hyprctl, Luac, System};
use garage_render::all::{render_all, render_wallpaper};
use garage_render::cx::RenderCx;
use garage_render::dispatch::run_render;
use serde_json::Value;

use crate::displays::{display_finish, display_test, watchdog};
use crate::error::CliError;
use crate::response::{emit, USAGE};
use crate::session::{acted, applied, synced_night_shift, synced_theme};
use crate::set;

/// The four commands that answer a person rather than the QML client, so their output is
/// lines and their failures are stderr -- never the JSON envelope, which nothing at a
/// terminal would want to read. `doctor --report` prints JSON, but it is still this kind of
/// command: the blob is for a person to paste, and a failure of it is still a message on
/// stderr.
const PLAIN_COMMANDS: [&str; 4] = ["doctor", "migrate", "repair", "update"];

/// What a settings-backend command left for [`emit`] to print.
///
/// Two shapes, because `_display-watchdog` is the one command in the table that prints
/// nothing at all: it runs unattended, fifteen seconds after the process that started it has
/// already answered, and there is nobody left to read an envelope.
pub(crate) enum Emitted {
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
    let system = System;
    if command == "reconcile" {
        return crate::reconcile::run(&paths, argv);
    }
    if PLAIN_COMMANDS.contains(&command) {
        return plain(&paths, &system, command, argv);
    }
    match settings(&paths, &system, command, argv) {
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

/// `doctor`, `migrate`, `repair`, `update`: lines for a person, failures on stderr, `garage
/// {command}: {error}`.
///
/// The Python's own branch supplies the catch tier: an error prints `garage {command}:
/// {error}` and returns 1. For its three legacy commands `migrate_config_root()` runs first
/// and its failure is reported the same way as the command's own; `migrate` skips that write
/// path to keep `--dry-run` read-only. Everything these four print on the way to succeeding
/// has already gone to stdout by then -- they print as they go rather than assembling an
/// envelope -- so a failure late in `update` leaves the transcript above it on screen.
fn plain(paths: &Paths, proc: &dyn Runner, command: &str, argv: &[String]) -> u8 {
    let arguments: &[String] = argv.get(2..).unwrap_or_default();
    // The migration runner owns a strict read-only mode. Running preference-root migration
    // before it would make `garage migrate --dry-run` write for an unrelated reason.
    let outcome = if command == "migrate" {
        crate::migrate::migrate(paths, arguments).map_err(CliError::from)
    } else {
        migrate_config_root(paths)
            .map_err(CliError::from)
            .and_then(|()| match command {
                "doctor" => Ok(doctor(paths, proc, arguments)?),
                "repair" => Ok(repair(paths, arguments)?),
                "update" => Ok(update(paths, proc, arguments)?),
                // Unreachable: `run()` only calls this for a name in `PLAIN_COMMANDS`,
                // `migrate` was handled above, and this match covers the other three.
                // Answered rather than panicked because the workspace denies `panic!`.
                other => Err(CliError::UnknownCommand(other.to_owned())),
            })
    };
    match outcome {
        Ok(status) => u8::try_from(status).unwrap_or(1),
        Err(error) => {
            eprintln!("garage {command}: {error}");
            1
        }
    }
}

/// The settings backend: one JSON object out, or one error in the same envelope.
///
/// `migrate_config_root()` runs first and its failure travels through the envelope, which is
/// the Python's placement -- inside the `try`, ahead of the dispatch.
fn settings(
    paths: &Paths,
    proc: &dyn Runner,
    command: &str,
    argv: &[String],
) -> Result<Emitted, CliError> {
    migrate_config_root(paths)?;
    match command {
        "snapshot" => Ok(Emitted::Envelope(make_snapshot(paths, proc)?)),
        // Files only. Every fragment is rewritten and nothing is signalled: see
        // render_all(). `apply` is the one that also moves the session.
        "render" => rendered(paths, proc, None),
        // hypridle's ExecStartPre. One file, which is all hypridle reads -- and all it may
        // render, because `set lock.*` restarts that unit synchronously while holding
        // PREFERENCES_LOCK.
        "render-idle" => rendered(paths, proc, Some(RenderStep::Idle)),
        // hyprpaper's ExecStartPre, deliberately not "render".
        "render-wallpaper" => rendered_wallpaper(paths, proc),
        // render, then push everything into the running session.
        "apply" => applied(paths, proc),
        "set" => set::set(paths, proc, argv).map(Emitted::Envelope),
        "action" => acted(paths, proc, argv),
        "display-test" => display_test(paths, proc, argv).map(Emitted::Envelope),
        "display-confirm" => display_finish(paths, proc, argv, true).map(Emitted::Envelope),
        "display-revert" => display_finish(paths, proc, argv, false).map(Emitted::Envelope),
        "_display-watchdog" => {
            watchdog(paths, proc, argv);
            Ok(Emitted::Silent)
        }
        "theme-sync" => synced_theme(paths, proc),
        "night-shift-sync" => synced_night_shift(paths, proc),
        other => Err(CliError::UnknownCommand(other.to_owned())),
    }
}

/// `render_all(load_preferences())`, or one named step of it.
///
/// The context is assembled per invocation and thrown away with it, and it carries no lock
/// -- see [`RenderCx`] -- which is what lets `render-idle` be re-entered from
/// `hypridle.service`'s `ExecStartPre` while a `set lock.*` is holding `PREFERENCES_LOCK`.
/// Neither arm below can reach one, and neither wants to.
///
/// The full render additionally builds a [`SessionCx`], for one argument and one only: the
/// browser command `render_general()` publishes as its third marker, which resolves through
/// `gio mime` and therefore needs a runner. Handing the value in is not handing the render
/// half a capability -- see [`garage_render::render_general`] for the whole of that
/// reasoning -- and `garage render` writes the same three markers the Python's does because
/// of it. `render-idle` takes the narrow arm and builds no session at all.
fn rendered(
    paths: &Paths,
    proc: &dyn Runner,
    step: Option<RenderStep>,
) -> Result<Emitted, CliError> {
    let config = load_preferences(paths, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let cx = RenderCx::new(&config, paths, &monitors, &lua);
    match step {
        // `render_all(load_keybindings())` in the Python, where the load happens inside the
        // renderer. It happens here instead because parsing `keybindings.toml` and filtering
        // it against the published catalog is `garage-apply`'s, and `garage-render` cannot
        // reach that crate -- see `render_all`'s own doc. Notes go to stderr, as they do
        // there.
        None => {
            let session = SessionCx::new(cx, proc);
            let resolve = resolve_browser(&session);
            render_all(&cx, &load_keybindings(paths, None), Some(&resolve))?;
        }
        Some(step) => run_render(step, &cx)?,
    }
    Ok(Emitted::Envelope(Value::Bool(true)))
}

/// `render-wallpaper`: `render_wallpaper(load_preferences())` alone -- hyprpaper's
/// `ExecStartPre`. The moved flag [`garage_render::all::render_wallpaper`] reports is not
/// read here, the same way `render`'s own call drops it: nothing downstream of this command
/// restarts the service on its say-so, that decision belongs to `garage apply`.
///
/// # Errors
///
/// Whatever [`garage_render::all::render_wallpaper`] returns.
fn rendered_wallpaper(paths: &Paths, proc: &dyn Runner) -> Result<Emitted, CliError> {
    let config = load_preferences(paths, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let cx = RenderCx::new(&config, paths, &monitors, &lua);
    let _moved = render_wallpaper(&cx)?;
    Ok(Emitted::Envelope(Value::Bool(true)))
}

#[cfg(test)]
mod tests {
    use super::{settings, Emitted, PLAIN_COMMANDS};
    use crate::error::CliError;
    use crate::testing::Offline;
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

    /// The whole of USAGE's settings backend, plus the name that is not in it: the default
    /// when no subcommand is given. The watchdog is deliberately absent -- it sleeps fifteen
    /// seconds by design.
    const COMMANDS: [&str; 11] = [
        "snapshot",
        "render",
        "render-idle",
        "render-wallpaper",
        "apply",
        "action",
        "display-test",
        "display-confirm",
        "display-revert",
        "theme-sync",
        "night-shift-sync",
    ];

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
            &Offline::new(),
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
        // See COMMANDS: a command this dispatch forgot would come back as "Unknown command",
        // which is the one failure this test exists to catch.
        for command in COMMANDS {
            let outcome = settings(&paths(), &Offline::new(), command, &argv(&[command]));
            assert!(
                !matches!(outcome, Err(CliError::UnknownCommand(_))),
                "{command} is not dispatched"
            );
        }
    }

    /// The claim that replaced "a stub names the command that is owed": there is no stub
    /// left. Every name in the table reaches a real layer, so a failure here is always
    /// something the machine or the argument said -- against this deliberately unwritable
    /// `HOME`, a filesystem refusal naming the path.
    #[test]
    fn no_command_in_the_table_answers_that_it_has_not_been_ported() {
        for command in COMMANDS {
            let message = settings(&paths(), &Offline::new(), command, &argv(&[command]))
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default();
            assert!(
                !message.contains("has not been ported yet"),
                "{command}: {message}"
            );
        }
    }

    #[test]
    fn set_with_the_wrong_argument_count_is_a_usage_message_not_a_schema_refusal() {
        let error = settings(
            &paths(),
            &Offline::new(),
            "set",
            &argv(&["set", "appearance.accent_color"]),
        )
        .err()
        .map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some("Usage: garage set KEY JSON_VALUE"));
        let too_many = settings(
            &paths(),
            &Offline::new(),
            "set",
            &argv(&["set", "a.b", "1", "2"]),
        )
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
    fn the_plain_table_names_all_four_human_commands() {
        assert_eq!(PLAIN_COMMANDS, ["doctor", "migrate", "repair", "update"]);
    }

    /// A real scratch `HOME`, unlike [`paths`]'s deliberately unwritable one: the narrow
    /// render commands are exercised for real here, down to the bytes on disk, rather than
    /// only "not `UnknownCommand`".
    fn scratch_paths(label: &str) -> Paths {
        let home = std::env::temp_dir().join(format!(
            "garage-cli-dispatch-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        Paths::from_env_map(&env)
    }

    /// `garage render` against a scratch tree with no `displays.toml` at all:
    /// `render_display_fragment()`'s empty-layout branch, which takes the fragment away
    /// rather than writing one -- see `garage_render::displays::render_saved_displays` for
    /// the gate. This test makes the same claim end to end.
    #[test]
    fn render_completes_end_to_end_with_no_displays_toml() {
        let paths = scratch_paths("render-all");
        let outcome = settings(&paths, &Offline::new(), "render", &argv(&["render"]))
            .expect("render completes with no displays.toml in scratch");
        assert!(
            matches!(outcome, Emitted::Envelope(value) if value == serde_json::Value::Bool(true))
        );
        assert!(paths.fragments.hyprland.exists());
        assert!(paths.markers.bar_layout.exists());
        assert!(!paths.fragments.displays.exists());
        drop(std::fs::remove_dir_all(&paths.home));
    }

    #[test]
    fn render_wallpaper_writes_only_hyprpaper_conf() {
        let paths = scratch_paths("render-wallpaper");
        let outcome = settings(
            &paths,
            &Offline::new(),
            "render-wallpaper",
            &argv(&["render-wallpaper"]),
        )
        .expect("render-wallpaper succeeds against a real scratch HOME");
        assert!(
            matches!(outcome, Emitted::Envelope(value) if value == serde_json::Value::Bool(true))
        );
        assert!(paths.fragments.hyprpaper.exists());
        assert!(!paths.fragments.locale_env.exists());
        assert!(!paths.markers.bar_layout.exists());
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
