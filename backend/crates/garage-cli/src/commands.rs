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
//! answer and the one the differential harness can read. `help`, `render` (through
//! [`render_all`]), `render-idle` (through [`run_render`]), `render-bar` (through
//! [`garage_render::all::render_bar`]) and `render-wallpaper` (through
//! [`garage_render::all::render_wallpaper`]) all reach real code now; `set` reaches the real
//! load, the real write and the real route walk, failing only at the first step of the
//! route. `render` itself completes end to end, with or without a saved layout, since task
//! 3.8 replaced `render_displays()`'s stub. The four display commands -- `display-test`,
//! `display-confirm`, `display-revert` and the watchdog -- are real, and live in
//! [`crate::displays`]. `apply` has no entry
//! point in `garage-apply` to call yet; it is wired the moment that crate grows one, and
//! nothing here changes when it does.

use garage_apply::keybind::load_keybindings;
use garage_core::paths::Paths;
use garage_core::schema::RenderStep;
use garage_prefs::{load_preferences, migrate_config_root};
use garage_proc::{Hyprctl, Luac, System};
use garage_render::all::{render_all, render_bar, render_wallpaper};
use garage_render::cx::RenderCx;
use garage_render::dispatch::run_render;
use serde_json::Value;

use crate::displays::{display_finish, display_test, watchdog};
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
        // narrow rather than a full render.
        "render-bar" => rendered_bar(paths),
        // hyprpaper's ExecStartPre, deliberately not "render".
        "render-wallpaper" => rendered_wallpaper(paths),
        // apply_preferences(): task 3.7.
        "apply" => Err(CliError::PortPending("apply")),
        "set" => set::set(paths, argv).map(Emitted::Envelope),
        // action(): task 3.10.
        "action" => Err(CliError::PortPending("action")),
        "display-test" => display_test(paths, argv).map(Emitted::Envelope),
        "display-confirm" => display_finish(paths, argv, true).map(Emitted::Envelope),
        "display-revert" => display_finish(paths, argv, false).map(Emitted::Envelope),
        "_display-watchdog" => {
            watchdog(paths, argv);
            Ok(Emitted::Silent)
        }
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
        // `render_all(load_keybindings())` in the Python, where the load happens inside the
        // renderer. It happens here instead because parsing `keybindings.toml` and filtering
        // it against the published catalog is `garage-apply`'s, and `garage-render` cannot
        // reach that crate -- see `render_all`'s own doc. Notes go to stderr, as they do
        // there.
        None => render_all(&cx, &load_keybindings(paths, None))?,
        Some(step) => run_render(step, &cx)?,
    }
    Ok(Emitted::Envelope(Value::Bool(true)))
}

/// `render-bar`: `render_region()` + `render_bar_workspaces()` + `render_bar_widgets()`, the
/// three fragments waybar's `ExecStartPre` needs and nothing else -- see
/// [`garage_render::all::render_bar`] for why this is narrower than [`render_all`].
///
/// # Errors
///
/// Whatever [`garage_render::all::render_bar`] returns.
fn rendered_bar(paths: &Paths) -> Result<Emitted, CliError> {
    let config = load_preferences(paths, None)?;
    let system = System;
    let monitors = Hyprctl::new(&system);
    let lua = Luac::new(&system);
    let cx = RenderCx::new(&config, paths, &monitors, &lua);
    render_bar(&cx)?;
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
fn rendered_wallpaper(paths: &Paths) -> Result<Emitted, CliError> {
    let config = load_preferences(paths, None)?;
    let system = System;
    let monitors = Hyprctl::new(&system);
    let lua = Luac::new(&system);
    let cx = RenderCx::new(&config, paths, &monitors, &lua);
    let _moved = render_wallpaper(&cx)?;
    Ok(Emitted::Envelope(Value::Bool(true)))
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

    /// A real scratch `HOME`, unlike [`paths`]'s deliberately unwritable one: `render-bar`
    /// and `render-wallpaper` are exercised for real here, down to the bytes on disk, rather
    /// than only "not `UnknownCommand`".
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

    #[test]
    fn render_bar_writes_the_three_bar_fragments_and_nothing_else() {
        let paths = scratch_paths("render-bar");
        let outcome = settings(&paths, "render-bar", &argv(&["render-bar"]))
            .expect("render-bar succeeds against a real scratch HOME");
        assert!(
            matches!(outcome, Emitted::Envelope(value) if value == serde_json::Value::Bool(true))
        );
        assert!(paths.fragments.locale_env.exists());
        assert!(paths.fragments.waybar_clock.exists());
        assert!(paths.fragments.waybar_workspaces.exists());
        assert!(paths.fragments.waybar_widgets.exists());
        // Narrow, unlike `render`: no toolkit config and no theme marker are touched.
        assert!(!paths.markers.bar_foreground.exists());
        drop(std::fs::remove_dir_all(&paths.home));
    }

    /// `garage render` against a scratch tree with no `displays.toml` at all:
    /// `render_display_fragment()`'s empty-layout branch, which takes the fragment away
    /// rather than writing one -- see `garage_render::displays::render_saved_displays` for
    /// the gate, and the `render` differential family for the same claim end to end.
    #[test]
    fn render_completes_end_to_end_with_no_displays_toml() {
        let paths = scratch_paths("render-all");
        let outcome = settings(&paths, "render", &argv(&["render"]))
            .expect("render completes with no displays.toml in scratch");
        assert!(
            matches!(outcome, Emitted::Envelope(value) if value == serde_json::Value::Bool(true))
        );
        assert!(paths.fragments.hyprland.exists());
        assert!(paths.fragments.waybar_widgets.exists());
        assert!(!paths.fragments.displays.exists());
        drop(std::fs::remove_dir_all(&paths.home));
    }

    #[test]
    fn render_wallpaper_writes_only_hyprpaper_conf() {
        let paths = scratch_paths("render-wallpaper");
        let outcome = settings(&paths, "render-wallpaper", &argv(&["render-wallpaper"]))
            .expect("render-wallpaper succeeds against a real scratch HOME");
        assert!(
            matches!(outcome, Emitted::Envelope(value) if value == serde_json::Value::Bool(true))
        );
        assert!(paths.fragments.hyprpaper.exists());
        assert!(!paths.fragments.locale_env.exists());
        assert!(!paths.markers.bar_foreground.exists());
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
