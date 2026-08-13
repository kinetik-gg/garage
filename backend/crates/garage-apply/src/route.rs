//! Walking a route: `apply_preferences()`, `apply_changed_preference()`, and the two step
//! kinds that have no module of their own to live in.
//!
//! `apply_changed_preference()` is the whole reason [`garage_core::schema::routes`] exists as
//! a typed table rather than a ladder of string prefixes. The Python version used to be
//! exactly that ladder -- `key.startswith("glass_")` and so on -- which is what let a new key
//! be added, saved, and then silently never applied, because the ladder's final `else` was
//! the only thing that would have said so. `Route::steps()` and this crate's
//! [`crate::dispatch::run_apply`] replace the ladder with an exhaustive match the compiler
//! enforces instead of a fallthrough a person has to remember to update.
//!
//! `apply_preferences()` is the session-start path, from `autostart.lua`, and the only
//! command that is supposed to touch every subsystem at once: render everything, then push
//! accent, corner radius and theme, reload the compositor, apply the wallpaper and the night
//! shift schedule, and -- unless told not to, which only the idle-route's own re-entrant
//! render asks for -- restart `hypridle.service`. `render` is the pure half of this same
//! sequence.
//!
//! # The two steps with no dedicated module
//!
//! [`ApplyStep::RunOrRaise`](garage_core::schema::routes::ApplyStep::RunOrRaise) is not a
//! named Python function call in the same sense as the rest of the table -- it is the one
//! step whose whole behaviour (which command, which failure message) is carried as data on
//! the step itself. `run_or_raise()` is what turns that data into a call: run the command,
//! and raise with the message if it fails. It lives here because it is route-table plumbing
//! in exactly the sense `apply_changed_preference()` is, not because it is thematically
//! closer to route-walking than to any other applier.
//!
//! [`ApplyStep::Accent`](garage_core::schema::routes::ApplyStep::Accent) has no dedicated
//! module in this crate's map: the Python's `apply_accent()` is `render_accent()` (the file
//! half, ported to [`garage_render::accent`]) followed by `push_accent()` (the `gsettings`
//! push). Both are two or three lines each, and neither is named as its own file in the
//! approved module split, so their apply-side stub is kept here rather than inventing a file
//! the plan does not call for.

use garage_core::schema::routes::{RenderStep, Step};
use garage_core::schema::{PreferenceKey, Route, Section};
use garage_render::all::{render_all, render_wallpaper};
use garage_render::dispatch::run_render;
use garage_render::render_accent;

use crate::dispatch::run_apply;

use crate::command::run;
use crate::corner::push_corner_radius;
use crate::cx::SessionCx;
use crate::displays::transaction::initialize_display_config;
use crate::error::ApplyError;
use crate::keybind::load_keybindings;
use crate::night_shift::apply_night_shift;
use crate::terminal::resolve_browser;
use crate::theme::push_theme;
use crate::wallpaper::{apply_wallpaper, Moved};

/// `apply_changed_preference()`'s dispatch, minus the walk: which route one changed key takes
/// (garage:4923-4943).
///
/// The two refusals below cannot be reached from a parsed [`PreferenceKey`] today, and they
/// are written down anyway because the Python's own fallback cannot be reached either -- a key
/// that is in `PREFERENCE_SCHEMA` always has a `route`, and a key that is not never survives
/// `set_nested()`. What `SECTION_ROUTES` and these two messages describe is the behaviour a
/// key *outside* the schema has always had, and the Python's comment is explicit that
/// appearance, general and bar name the key while the rest name the section. Dropping either
/// half here would be a refactor inventing an error the product never had.
///
/// # Errors
///
/// [`ApplyError::UnsupportedPreference`] or [`ApplyError::UnsupportedSection`] for a key that
/// routes nowhere, chosen by the same three-section split the Python makes.
pub fn route_for(key: PreferenceKey) -> Result<Route, ApplyError> {
    if let Some(route) = key.route() {
        return Ok(route);
    }
    let section = key.section();
    if let Some(route) = section.route() {
        return Ok(route);
    }
    match section {
        Section::Appearance | Section::General | Section::Bar => {
            Err(ApplyError::UnsupportedPreference {
                section,
                key: key.name().to_owned(),
            })
        }
        Section::Indexing
        | Section::Input
        | Section::Lock
        | Section::Region
        | Section::Workspaces => Err(ApplyError::UnsupportedSection(section)),
    }
}

/// Move the running session onto one changed key, and nothing else (garage:4915-4943).
///
/// `for step in PREFERENCE_ROUTES[route]: globals()[name](config, *arguments)`, with the
/// render half reached through the [`RenderCx`](garage_render::cx::RenderCx) this context
/// contains -- which is what keeps a render on this path structurally unable to take the
/// preferences lock the caller is holding.
///
/// # `render_general` is the one step this walk does not hand to `garage-render`
///
/// The Python's `render_general()` writes three markers: the launcher, the terminal and the
/// browser. [`garage_render::render_general`] writes the first two and cannot write the
/// third -- resolving a browser association runs `gio mime` three times, and a `RenderCx`
/// structurally carries no runner. Here there *is* one, and this is a walk of the Python's
/// own route table, so `RenderStep::General` reaches
/// [`crate::terminal::publish_general`] instead: the same two markers from the same
/// renderer, then the third, in the Python's own order. A route that is supposed to publish
/// what `binds.lua` reads must publish all of it.
///
/// `garage render` is the one caller that still cannot -- it builds no session context by
/// design -- and that difference is written down in `tests/differential/deviations.toml`
/// against the `render-all-empty` scenario.
///
/// # Errors
///
/// Whatever [`route_for`] refuses, then the first step of the route to fail.
pub fn apply_changed_preference(
    cx: &mut SessionCx<'_>,
    key: PreferenceKey,
) -> Result<(), ApplyError> {
    for step in route_for(key)?.steps() {
        match *step {
            Step::Render(RenderStep::General) => crate::terminal::publish_general(cx)?,
            Step::Render(step) => run_render(step, cx.render())?,
            Step::Apply(step) => run_apply(step, cx)?,
        }
    }
    Ok(())
}

/// Render everything, then move the whole running session onto it. The session-start path
/// (garage:4814-4838).
///
/// `restart_idle` is the Python's keyword-only argument of the same name, kept rather than
/// folded away: it is the one knob this function has, and the caller that would pass `false`
/// -- a re-entrant apply from inside the idle route -- is the reason `hypridle.service`'s
/// `ExecStartPre` is `garage render-idle` and not `garage render` at all. `garage apply`
/// passes `true`.
///
/// # Errors
///
/// [`ApplyError::Render`] if any renderer fails, whatever `initialize_display_config()`,
/// `push_theme()` or `apply_wallpaper()` refuse. The four bare signals -- the reload, the
/// night shift push and the idle restart -- report nothing, as they do there.
pub fn apply_preferences(cx: &mut SessionCx<'_>, restart_idle: bool) -> Result<(), ApplyError> {
    // First, so that on a machine which has never had a saved layout the workspace plan and
    // the display fragment are both rendered from one -- the same one, describing the
    // monitors actually attached. On the apply side because it is the only part of a render
    // that needs a live compositor to answer, and because session start is the moment it
    // exists for.
    let primary = std::env::var("HYPR_PRIMARY_MONITOR").unwrap_or_default();
    initialize_display_config(cx, &primary)?;
    // Ahead of render_all(), which writes the same file: the restart below is owed exactly
    // when hyprpaper.conf moved, and only the first writer can see that it did. hyprpaper
    // reads fit_mode once, at startup, and exposes no reload.
    let paper_moved = render_wallpaper(cx.render())?;
    let document = load_keybindings(cx.render().paths(), None);
    // The browser marker's resolver: `render_all()` writes all three general markers on this
    // path, exactly as the Python's does, because this caller holds a runner. See
    // [`garage_render::render_general`] for the whole of why the value travels rather than
    // the capability.
    {
        let session: &SessionCx<'_> = cx;
        let resolve = resolve_browser(session);
        render_all(session.render(), &document, Some(&resolve))?;
    }
    push_accent(cx);
    push_corner_radius(cx);
    push_theme(cx)?;
    drop(run(cx, &["hyprctl", "reload"]));
    apply_wallpaper(cx, Moved::Known(paper_moved))?;
    let _took = apply_night_shift(cx);
    if restart_idle {
        drop(run(
            cx,
            &["systemctl", "--user", "restart", "hypridle.service"],
        ));
    }
    Ok(())
}

/// Run a route step's command, and fail with its message if the command fails
/// (garage:4861-4868).
///
/// The Python's `config` argument is absent here for the reason its own docstring gives for
/// carrying one: "every step in `PREFERENCE_ROUTES` is called with it, so the route table can
/// stay data rather than becoming a table of closures". The Rust route table already *is*
/// data -- [`ApplyStep::RunOrRaise`](garage_core::schema::routes::ApplyStep::RunOrRaise)
/// carries the argv and the message on the step -- so the parameter that existed to keep it
/// uniform has nothing left to be uniform with.
///
/// # Errors
///
/// [`ApplyError::Signal`] carrying the command's own complaint when it had one, and the
/// step's message when it did not. Both halves reach the user's stderr verbatim, which is why
/// the choice between them is made here rather than by whoever prints it.
pub(crate) fn run_or_raise(
    cx: &SessionCx<'_>,
    command: &[&str],
    message: &str,
) -> Result<(), ApplyError> {
    let result = run(cx, command);
    if result.status == 0 {
        return Ok(());
    }
    let detail = result.stderr.trim();
    Err(ApplyError::Signal(if detail.is_empty() {
        message.to_owned()
    } else {
        detail.to_owned()
    }))
}

/// Push the accent into GNOME's interface settings. The world half (garage:2404-2409).
///
/// Gated on `gsettings range`, because the key only exists from GNOME 47 and the accent names
/// it accepts are a closed set: setting one it does not have is an error on stderr and a
/// no-op, so the range is read first and the push is skipped when the name is not in it.
pub(crate) fn push_accent(cx: &mut SessionCx<'_>) {
    let accent = cx.render().prefs().appearance.accent_color.as_str();
    let range = run(
        cx,
        &[
            "gsettings",
            "range",
            "org.gnome.desktop.interface",
            "accent-color",
        ],
    );
    if range.status == 0 && range.stdout.contains(&format!("'{accent}'")) {
        drop(run(
            cx,
            &[
                "gsettings",
                "set",
                "org.gnome.desktop.interface",
                "accent-color",
                accent,
            ],
        ));
    }
}

/// Publish the accent marker, then push it into GNOME's interface settings (garage:2412-2414).
/// See the module doc for why this orphan step lives here.
///
/// # Errors
///
/// [`ApplyError::Render`] if the accent marker could not be written. The push reports nothing.
pub(crate) fn apply_accent(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_accent(cx.render())?;
    push_accent(cx);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_accent, apply_preferences, run_or_raise};
    use crate::testing::{Script, World};

    #[test]
    fn the_accent_is_only_pushed_when_gsettings_admits_to_the_name() {
        let world = World::new(
            "accent-out-of-range",
            "[appearance]\naccent_color = \"teal\"\n",
            Script::new().answering(
                "gsettings range org.gnome.desktop.interface accent-color",
                0,
                "enum\n'blue'\n'green'\n",
                "",
            ),
        );
        world.with(|cx| apply_accent(cx).expect("the marker is written"));
        assert_eq!(
            world.signals(),
            ["gsettings range org.gnome.desktop.interface accent-color"]
        );
    }

    #[test]
    fn an_accent_in_range_is_set() {
        let world = World::new(
            "accent-in-range",
            "[appearance]\naccent_color = \"teal\"\n",
            Script::new().answering(
                "gsettings range org.gnome.desktop.interface accent-color",
                0,
                "enum\n'teal'\n'blue'\n",
                "",
            ),
        );
        world.with(|cx| apply_accent(cx).expect("the marker is written"));
        assert_eq!(
            world.signals().last().map(String::as_str),
            Some("gsettings set org.gnome.desktop.interface accent-color teal")
        );
    }

    #[test]
    fn run_or_raise_prefers_the_commands_own_complaint() {
        let world = World::plain(
            "run-or-raise",
            Script::new().answering("hyprctl reload", 1, "", "  Couldn't reload  \n"),
        );
        world.with(|cx| {
            let error = run_or_raise(cx, &["hyprctl", "reload"], "Unable to reload theme")
                .expect_err("the reload was refused");
            assert_eq!(error.to_string(), "Couldn't reload");
        });
    }

    #[test]
    fn the_session_start_path_signals_in_the_pythons_order() {
        let world = World::new(
            "apply-preferences",
            "[appearance]\ntheme_mode = \"dark\"\n",
            Script::new(),
        );
        world.with(|cx| apply_preferences(cx, true).expect("the session-start path completes"));
        let signals = world.signals();
        let positions = |needle: &str| {
            signals
                .iter()
                .position(|line| line.starts_with(needle))
                .unwrap_or(usize::MAX)
        };
        // accent, then corner radius, then the theme's own seven, then the reload, then the
        // wallpaper decision, the night shift push and the idle restart.
        assert!(positions("gsettings range") < positions("hyprctl eval hl.config({decoration"));
        assert!(positions("hyprctl eval hl.config({decoration") < positions("pkill -USR2"));
        assert!(positions("pkill -USR2") < positions("hyprctl reload"));
        assert!(positions("hyprctl reload") < positions("hyprctl hyprsunset"));
        assert_eq!(
            signals.last().map(String::as_str),
            Some("systemctl --user restart hypridle.service")
        );
    }

    #[test]
    fn the_idle_restart_is_the_one_step_a_caller_can_decline() {
        let world = World::new("apply-no-idle", "", Script::new());
        world.with(|cx| apply_preferences(cx, false).expect("completes"));
        assert!(!world
            .trace()
            .iter()
            .any(|line| line.contains("hypridle.service")));
    }
}
