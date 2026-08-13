//! The four commands that move the running session: `apply`, `action`, `theme-sync` and
//! `night-shift-sync`.
//!
//! Split out of [`crate::commands`] for the reason [`crate::displays`] is: each of these
//! assembles a [`SessionCx`] and hands the whole thing to `garage-apply`, where the
//! render-only commands beside them build a bare `RenderCx` and stop there. Keeping the two
//! kinds apart is the same one-way relation the two crates have, made visible in the file
//! layout: nothing in [`crate::commands`]' own body can reach a runner by accident.
//!
//! `action` is the odd one and deliberately so -- it builds no context at all. `main()`'s
//! `action` branch never loads the preferences in the Python, and two of the action arms load
//! them themselves under the lock, so the entry point takes `paths` and a runner and lets
//! `garage-apply` decide. See `garage_apply::action` for why that matters: a load is not free
//! of consequence, it compacts the file it read.

use garage_apply::{action, apply_night_shift, apply_preferences, theme_sync, SessionCx};
use garage_core::paths::Paths;
use garage_core::traits::Runner;
use garage_prefs::load_preferences;
use garage_proc::{Hyprctl, Luac};
use garage_render::cx::RenderCx;
use garage_render::theme::night_shift_active;
use serde_json::{json, Value};

use crate::commands::Emitted;
use crate::error::CliError;

/// `garage apply`: `apply_preferences(load_preferences())`, then `response(True)`.
///
/// The one command that is supposed to touch every subsystem at once, and the one
/// `autostart.lua` runs at session start.
///
/// # Errors
///
/// Whatever the load or [`apply_preferences`] refuses.
pub(crate) fn applied(paths: &Paths, proc: &dyn Runner) -> Result<Emitted, CliError> {
    let config = load_preferences(paths, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let mut cx = SessionCx::new(RenderCx::new(&config, paths, &monitors, &lua), proc);
    apply_preferences(&mut cx, true)?;
    Ok(Emitted::Envelope(Value::Bool(true)))
}

/// `action NAME [JSON]`: `action(argv[2], json.loads(argv[3]) if len(argv) > 3 else None)`,
/// then `response(True)`.
///
/// No preferences are loaded here, which is the Python's shape and not an omission: two of
/// the arms load them themselves, under the lock, and the rest read none at all.
///
/// **Deviation, stated plainly:** `garage action` with no name raises `IndexError` in the
/// Python -- uncaught, so a traceback on stderr and exit 1 -- because `argv[2]` is indexed
/// rather than checked. Here it is the envelope's own refusal, the same shape the four
/// `display-*` commands already use. The exit status is 1 either way.
///
/// # Errors
///
/// [`CliError::DisplayUsage`] for a missing name, [`CliError::Json`] if the payload does not
/// parse, and whatever the action itself refuses.
pub(crate) fn acted(
    paths: &Paths,
    proc: &dyn Runner,
    argv: &[String],
) -> Result<Emitted, CliError> {
    let name = argv
        .get(2)
        .ok_or(CliError::DisplayUsage("garage action NAME [JSON]"))?;
    let value = match argv.get(3) {
        Some(text) => Some(serde_json::from_str::<Value>(text)?),
        None => None,
    };
    action(paths, proc, name, value.as_ref())?;
    Ok(Emitted::Envelope(Value::Bool(true)))
}

/// `theme-sync`: switch light/dark if the schedule says so, and answer with the scheme.
///
/// # Errors
///
/// Whatever the load or [`theme_sync`] refuses.
pub(crate) fn synced_theme(paths: &Paths, proc: &dyn Runner) -> Result<Emitted, CliError> {
    let config = load_preferences(paths, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let mut cx = SessionCx::new(RenderCx::new(&config, paths, &monitors, &lua), proc);
    let scheme = theme_sync(&mut cx)?;
    Ok(Emitted::Envelope(Value::String(scheme.as_str().to_owned())))
}

/// `night-shift-sync`: re-evaluate the window and answer with what it did.
///
/// One load, not three: the schedule is read against the clock, and a sync that straddled a
/// boundary would answer about a different minute than the one it applied.
///
/// # Errors
///
/// Whatever the load refuses. The push itself reports through the payload, not as an error --
/// hyprsunset not being up yet is exactly what this command exists to report.
pub(crate) fn synced_night_shift(paths: &Paths, proc: &dyn Runner) -> Result<Emitted, CliError> {
    let config = load_preferences(paths, None)?;
    let monitors = Hyprctl::new(proc);
    let lua = Luac::new(proc);
    let mut cx = SessionCx::new(RenderCx::new(&config, paths, &monitors, &lua), proc);
    let applied = apply_night_shift(&mut cx);
    Ok(Emitted::Envelope(json!({
        "active": night_shift_active(&config),
        "applied": applied,
    })))
}
