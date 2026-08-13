//! `action()`: run one one-shot action -- volume, mute, default sink/source, night shift
//! toggle, glass reset, a keybind edit, a default-application change, an immediate lock, a
//! date/time change, a Waybar panel click, or a pointer-aware session-menu action.
//!
//! A second dispatch table, deliberately separate from [`crate::route`]'s route walking: an
//! action is not a preference, has no key in `preferences.toml` and no
//! [`Route`](garage_core::schema::routes::Route) of its own. It exists for the things the
//! pane needs to do that are not "set a value and move the session onto it" -- most of these
//! either read from or write straight to the world (`wpctl`, `pactl`, `loginctl`) with
//! nothing to persist, or they delegate to a whole-file read-modify-write of their own
//! (`keybind.*` into [`crate::keybind::action`], `defaults.*` into
//! [`crate::desktopfiles::roles`]).
//!
//! `glass.reset` is the one action that *is* a read-modify-write under the preferences lock,
//! exactly like `set`: every `glass_*` key is walked back to its shipped default without a
//! list of its own to keep in step with the schema, because every material preference is
//! named with that prefix. It is safe to block on the lock here, unlike the load path, which
//! only ever tries it, because nothing this action does restarts a service whose
//! `ExecStartPre` re-enters this binary.
//!
//! `datetime.ntp` and `datetime.timezone` are spawned detached rather than run to completion:
//! neither reads a result, and both hand the actual work to `systemd-timedated` over the bus,
//! so waiting on them would stall the settings path for an answer nothing here uses.
//!
//! Doc-only: the real signature takes an action name and a JSON payload rather than this
//! crate's fixed `(cx: &mut SessionCx<'_>) -> Result<(), ApplyError>` shape, and is reached
//! through the `action` command's own dispatch, never through `Route::steps()`.

mod audio;
mod hyprland;
mod menu;
mod panel;
mod preference;
mod pyvalue;

use garage_core::paths::Paths;
use garage_core::schema::defaults::Defaults;
use garage_core::schema::PreferenceKey;
use garage_core::traits::Runner;
use garage_proc::{Hyprctl, Luac};
use garage_render::cx::RenderCx;
use serde_json::Value;

use crate::actions::preference::{glass_reset, toggle_boolean_preference};
use crate::actions::pyvalue::py_str;
use crate::command::{run, run_checked};
use crate::cx::SessionCx;
use crate::desktopfiles::roles::set_default_app;
use crate::error::ApplyError;
use crate::keybind::{keybind_action, KeybindRequest};

/// Run one one-shot action (garage:5346-5411).
///
/// Takes `paths` and a runner rather than a [`SessionCx`], which is the one place in this
/// crate that does. The reason is the Python's own shape: `main()`'s `action` branch never
/// loads the preferences, and two of the arms below load them *themselves*, under the lock,
/// because the value they are about to change is the value they have to read first. Handing
/// this function an already-built context would mean loading layer 2 for every action --
/// including `defaults.browser`, which reads no preference at all -- and a load is not free
/// of consequence here: it compacts the file it read.
///
/// # Errors
///
/// [`ApplyError::Settings`] with `f"Unknown action: {name}"` for a name nothing here answers,
/// and otherwise whatever the arm itself refuses.
pub fn action(
    paths: &Paths,
    proc: &dyn Runner,
    name: &str,
    value: Option<&Value>,
) -> Result<(), ApplyError> {
    if audio::NAMES.contains(&name) {
        return light(paths, proc, |cx| audio::audio_action(cx, name, value));
    }
    match name {
        "appearance.night_shift.toggle" => {
            toggle_boolean_preference(paths, proc, PreferenceKey::NightShiftEnabled).map(drop)
        }
        "glass.reset" => glass_reset(paths, proc),
        "lock.now" => light(paths, proc, |cx| {
            run_checked(cx, &["loginctl", "lock-session"]).map(drop)
        }),
        "datetime.ntp" => light(paths, proc, |cx| {
            spawn(cx, &["timedatectl", "set-ntp", ntp_word(value)])
        }),
        "datetime.timezone" => light(paths, proc, |cx| set_timezone(cx, value)),
        "menu.dismiss" => light(paths, proc, |cx| menu::dismiss(cx)),
        "menu.toggle" => light(paths, proc, |cx| menu::toggle(cx)),
        "panel.toggle" => light(paths, proc, |cx| panel::panel_toggle(cx, value)),
        // `name.split(".", 1)[1]`: everything after the first dot, which is the operation for
        // a keybind action and the role for a defaults one.
        other => match (
            other.strip_prefix("keybind."),
            other.strip_prefix("defaults."),
        ) {
            (Some(operation), _) => light(paths, proc, |cx| {
                Ok(keybind_action(cx, operation, &request(value))?)
            }),
            (_, Some(role)) => light(paths, proc, |cx| {
                Ok(set_default_app(cx, role, &py_str(value))?)
            }),
            (None, None) => Err(ApplyError::Settings(format!("Unknown action: {other}"))),
        },
    }
}

/// A context for the arms that need `paths` and a runner and no preference at all.
///
/// The preference slot is filled from the compiled-in defaults, which is a pure parse of a
/// string in the binary and touches no file. Nothing reached through this closure reads a
/// preference -- `wpctl`, `pactl`, `loginctl`, `timedatectl`, `hyprctl` and `qs` take their
/// argument from the payload or live session, `keybind_action()` reads `keybindings.toml`, and
/// `set_default_app()` reads `mimeapps.list` and the desktop files. Reading layer 2 to fill the
/// slot honestly would be the dishonest choice: it would compact the user's file as a side
/// effect of turning the volume down.
fn light<R>(
    paths: &Paths,
    proc: &dyn Runner,
    body: impl FnOnce(&mut SessionCx<'_>) -> Result<R, ApplyError>,
) -> Result<R, ApplyError> {
    let defaults = Defaults::compiled().map_err(|error| ApplyError::Settings(error.to_string()))?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let render = RenderCx::new(defaults.values(), paths, &monitors, &lua);
    let mut session = SessionCx::new(render, proc);
    body(&mut session)
}

/// `"true" if value else "false"`.
const fn ntp_word(value: Option<&Value>) -> &'static str {
    if py_truthy_const(value) {
        "true"
    } else {
        "false"
    }
}

/// [`py_truthy`] in a `const fn`, which cannot call it because it allocates nothing but does
/// match on a borrowed `Value`. Kept as a thin forwarder so the truthiness rule stays in one
/// place for every other caller.
const fn py_truthy_const(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        _ => true,
    }
}

/// `subprocess.Popen(..., start_new_session=True)`: hand it to `systemd-timedated` and do not
/// wait. Neither caller reads a result, and both hand the actual work to the bus, so waiting
/// would stall the settings path for an answer nothing here uses.
fn spawn(cx: &SessionCx<'_>, command: &[&str]) -> Result<(), ApplyError> {
    cx.proc()
        .spawn_detached(command)
        .map_err(|error| ApplyError::Io(error.detail))
}

/// `datetime.timezone` (garage:5399-5409): refuse a name `timedatectl` does not list, then
/// hand the change over.
///
/// The list is read every time rather than cached: it is `timedatectl`'s own, it is the same
/// list the pane populated its picker from, and a `tzdata` update between the two is exactly
/// the case the check exists for. A `timedatectl` that could not be run at all reports an
/// empty list, so *every* timezone is unknown -- which is the Python's behaviour as written,
/// and the safe direction to fail in.
fn set_timezone(cx: &SessionCx<'_>, value: Option<&Value>) -> Result<(), ApplyError> {
    let timezone = py_str(value);
    let listed = run(cx, &["timedatectl", "list-timezones"]).stdout;
    if !listed.lines().any(|line| line == timezone) {
        return Err(ApplyError::Settings("Unknown timezone".to_owned()));
    }
    spawn(cx, &["timedatectl", "set-timezone", &timezone])
}

/// `values = payload if isinstance(payload, dict) else {}`, read one string field at a time.
fn request(value: Option<&Value>) -> KeybindRequest<'_> {
    let field = |name: &str| {
        value
            .and_then(|payload| payload.get(name))
            .and_then(Value::as_str)
    };
    KeybindRequest {
        id: field("id"),
        keys: field("keys"),
        description: field("description"),
        command: field("command"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::action;
    use crate::testing::{Script, World};

    #[test]
    fn an_unknown_name_is_refused_in_the_pythons_words() {
        let world = World::plain("action-unknown", Script::new());
        let error =
            action(&world.paths, world.runner(), "nope.nothing", None).expect_err("no arm answers");
        assert_eq!(error.to_string(), "Unknown action: nope.nothing");
        assert!(world.trace().is_empty());
    }

    #[test]
    fn locking_now_goes_straight_to_logind() {
        let world = World::plain("action-lock", Script::new());
        action(&world.paths, world.runner(), "lock.now", None).expect("loginctl accepts");
        assert_eq!(world.trace(), ["loginctl lock-session"]);
    }

    #[test]
    fn the_ntp_switch_is_spawned_rather_than_waited_on() {
        let world = World::plain("action-ntp", Script::new());
        action(
            &world.paths,
            world.runner(),
            "datetime.ntp",
            Some(&json!(true)),
        )
        .expect("the spawn is accepted");
        assert_eq!(world.trace(), ["spawn: timedatectl set-ntp true"]);
    }

    #[test]
    fn a_timezone_the_machine_does_not_list_is_refused_before_anything_is_spawned() {
        let world = World::plain(
            "action-timezone-unknown",
            Script::new().answering(
                "timedatectl list-timezones",
                0,
                "Etc/UTC\nEurope/Amsterdam\n",
                "",
            ),
        );
        let error = action(
            &world.paths,
            world.runner(),
            "datetime.timezone",
            Some(&json!("Mars/Olympus")),
        )
        .expect_err("not a timezone");
        assert_eq!(error.to_string(), "Unknown timezone");
        assert_eq!(world.trace(), ["timedatectl list-timezones"]);
    }

    #[test]
    fn a_listed_timezone_is_handed_to_timedated() {
        let world = World::plain(
            "action-timezone",
            Script::new().answering(
                "timedatectl list-timezones",
                0,
                "Etc/UTC\nEurope/Amsterdam\n",
                "",
            ),
        );
        action(
            &world.paths,
            world.runner(),
            "datetime.timezone",
            Some(&json!("Europe/Amsterdam")),
        )
        .expect("the timezone is listed");
        assert_eq!(
            world.trace().last().map(String::as_str),
            Some("spawn: timedatectl set-timezone Europe/Amsterdam")
        );
    }

    #[test]
    fn a_default_application_for_a_role_nothing_defines_is_refused() {
        let world = World::plain("action-defaults-role", Script::new());
        let error = action(
            &world.paths,
            world.runner(),
            "defaults.spreadsheet",
            Some(&json!("gnumeric.desktop")),
        )
        .expect_err("no such role");
        assert_eq!(
            error.to_string(),
            "Unknown default application: spreadsheet"
        );
    }
}
