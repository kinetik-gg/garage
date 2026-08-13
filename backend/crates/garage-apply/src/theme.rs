//! `push_theme()`, `apply_theme()` and `apply_theme_if_scheme_moved()`: move the running
//! desktop onto the palette the renderer already wrote.
//!
//! `push_theme()` is the world half, and the one that publishes the scheme marker: from the
//! moment it runs, `applied_scheme()` answers with the scheme just pushed, which is true
//! because everything below it has been told about it too. It writes that marker before
//! anything else specifically so a caller reading it mid-push already sees the destination
//! rather than the source.
//!
//! The desktop picture belongs to the appearance, so it moves with the palette -- hooked here
//! because this is the one site every switch goes through: a manual change, session start,
//! and the five-minute theme timer. Gated on what `current` already resolves to rather than
//! on the scheme marker, because that marker was just overwritten a line above and can no
//! longer say what was on screen a moment ago; ungated, the timer would re-issue the
//! wallpaper -- and restart hyprpaper whenever the fit moved with it -- every five minutes
//! even when nothing changed.
//!
//! From there: GTK4 and libadwaita follow the portal setting live (`gsettings`), GTK3 and
//! `XWayland` apps re-theme when `xsettingsd` rereads its config (`reload-or-restart`), and
//! waybar, kitty and swayosd are each signalled to reread their own generated files.
//!
//! `apply_theme()` is render-then-push in one call: `render_theme()` followed by
//! `push_theme()`. `apply_theme_if_scheme_moved()` is the actual `Route::Theme` step, and the
//! one that matters for cost: rewriting a dozen toolkit configs and reloading Hyprland is
//! only worth doing when the palette actually moves, because nothing downstream reads the
//! mode or the schedule -- the renderer picks its decoration colours from the resolved scheme
//! too, so an unchanged scheme means every output would come out byte-identical. That also
//! covers switching to `auto` at night when dark is already live, since the theme timer
//! re-checks the schedule on its own five-minute interval regardless.

use garage_core::fs::marker::write_marker;
use garage_core::schema::enums::Scheme;
use garage_render::theme::{applied_scheme, resolve_theme};
use garage_render::{look, render_preferences, render_theme};

use crate::command::run;
use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::route::run_or_raise;
use crate::wallpaper::target::{current_wallpaper, wallpaper_target};
use crate::wallpaper::{apply_wallpaper, Moved};

/// Move the running desktop onto the palette `render_theme()` just wrote (garage:4653-4694).
///
/// # Errors
///
/// [`ApplyError::Marker`] if the scheme marker could not be written, or whatever
/// [`apply_wallpaper`] refuses once the picture has been decided to have moved. Every signal
/// below it is unchecked, exactly as the Python's seven bare `run()` calls are: a desktop
/// half-way onto a new palette is better than one left on the old one because `swayosd` was
/// not installed.
pub(crate) fn push_theme(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    let scheme = resolve_theme(cx.render().prefs());
    let names = look(scheme);

    // Quickshell watches this file, so its own UI repaints with no restart.
    write_marker(
        &cx.render().paths().markers.color_scheme,
        &format!("{}\n", scheme.as_str()),
    )?;

    // The desktop picture belongs to the appearance, so it moves with the palette. Hooked
    // here because this is the one site every switch goes through: a manual change, session
    // start, and the five-minute theme timer.
    //
    // Gated on what `current` already resolves to, not on applied_scheme(): the marker was
    // overwritten a line ago and can no longer say what was on screen. Ungated, the timer
    // would re-issue the wallpaper -- and restart hyprpaper whenever the fit moved with it --
    // every five minutes.
    //
    // A missing or unreadable picture is not a reason to abandon the rest of the palette; the
    // pane reports it the next time the setting is touched. That is the Python's
    // `except SettingsError: target = None`, and it is why this is `.ok().flatten()`.
    let target = wallpaper_target(cx, scheme).ok().flatten();
    if let Some(path) = target {
        let resolved = std::fs::canonicalize(&path).unwrap_or(path);
        if resolved.display().to_string() != current_wallpaper(cx.render().paths()) {
            apply_wallpaper(cx, Moved::Ask)?;
        }
    }

    // GTK4 and libadwaita follow the portal setting live.
    for (key, value) in [
        ("color-scheme", names.portal),
        ("gtk-theme", names.gtk),
        ("icon-theme", names.icons),
    ] {
        drop(run(
            cx,
            &[
                "gsettings",
                "set",
                "org.gnome.desktop.interface",
                key,
                value,
            ],
        ));
    }

    // GTK3 and XWayland apps re-theme when xsettingsd rereads its config.
    drop(run(
        cx,
        &[
            "systemctl",
            "--user",
            "reload-or-restart",
            "xsettingsd.service",
        ],
    ));

    // waybar rereads its stylesheet on SIGUSR2, kitty its whole config on SIGUSR1.
    drop(run(cx, &["pkill", "-USR2", "-x", "waybar"]));
    drop(run(cx, &["pkill", "-USR1", "-x", "kitty"]));
    drop(run(
        cx,
        &["systemctl", "--user", "try-restart", "swayosd.service"],
    ));
    Ok(())
}

/// Write the resolved palette everywhere, then move the desktop onto it (garage:4697-4700).
///
/// # Errors
///
/// Whatever [`render_theme`] or [`push_theme`] returns.
pub(crate) fn apply_theme(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_theme(cx.render())?;
    push_theme(cx)
}

/// Re-theme the session, but only when the resolved scheme has actually moved since the last
/// push (garage:4883-4894).
///
/// # Errors
///
/// Whatever [`apply_theme`] or [`render_preferences`] returns, and [`ApplyError::Signal`]
/// carrying `"Unable to reload theme"` if the compositor refuses the reload.
pub(crate) fn apply_theme_if_scheme_moved(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    if resolve_theme(cx.render().prefs()).as_str() == applied_scheme(cx.render().paths()) {
        return Ok(());
    }
    apply_theme(cx)?;
    render_preferences(cx.render())?;
    run_or_raise(cx, &["hyprctl", "reload"], "Unable to reload theme")
}

/// `theme-sync` (garage:6713-6719): the five-minute timer's own branch.
///
/// The same gate as the manual `theme_*` path, and *not* the same last step: this reloads the
/// compositor with a bare `run()` where [`apply_theme_if_scheme_moved`] uses `run_or_raise()`.
/// The difference is the caller. A `set` is a person waiting for an answer, so a refused
/// reload is worth reporting; a timer tick has nobody to tell, and turning one into an error
/// would make `garage-theme.timer` log a failure every five minutes on a machine whose
/// compositor is not up. Kept as the Python has it rather than unified.
///
/// Reports the resolved scheme either way, which is what the response carries.
///
/// # Errors
///
/// Whatever [`apply_theme`] or [`render_preferences`] returns.
pub fn theme_sync(cx: &mut SessionCx<'_>) -> Result<Scheme, ApplyError> {
    let scheme = resolve_theme(cx.render().prefs());
    // An unchanged scheme means every output would be byte-identical, so rewriting a dozen
    // toolkit configs and reloading Hyprland would only make the desktop visibly flicker every
    // five minutes for nothing.
    if scheme.as_str() != applied_scheme(cx.render().paths()) {
        apply_theme(cx)?;
        render_preferences(cx.render())?;
        drop(run(cx, &["hyprctl", "reload"]));
    }
    Ok(scheme)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{apply_theme_if_scheme_moved, push_theme};
    use crate::testing::{Script, World};

    /// The seven signals every push issues, in order, after the wallpaper decision.
    const SIGNALS: [&str; 7] = [
        "gsettings set org.gnome.desktop.interface color-scheme prefer-dark",
        "gsettings set org.gnome.desktop.interface gtk-theme adw-gtk3-dark",
        "gsettings set org.gnome.desktop.interface icon-theme Papirus-Dark",
        "systemctl --user reload-or-restart xsettingsd.service",
        "pkill -USR2 -x waybar",
        "pkill -USR1 -x kitty",
        "systemctl --user try-restart swayosd.service",
    ];

    #[test]
    fn the_scheme_marker_is_published_before_anything_else_is_told() {
        // "From the moment it runs, applied_scheme() answers with the scheme just pushed" --
        // so a caller reading it mid-push already sees the destination rather than the source.
        let world = World::new(
            "push-theme-marker",
            "[appearance]\ntheme_mode = \"dark\"\n",
            Script::new(),
        );
        world.with(|cx| push_theme(cx).expect("the push completes"));
        assert_eq!(
            fs::read_to_string(&world.paths.markers.color_scheme).expect("the marker was written"),
            "dark\n"
        );
        assert_eq!(world.signals(), SIGNALS);
    }

    #[test]
    fn an_unresolvable_picture_does_not_abandon_the_rest_of_the_palette() {
        let world = World::new(
            "push-theme-broken-picture",
            "[appearance]\ntheme_mode = \"dark\"\nwallpaper_dark = \"/nope/gone.png\"\n",
            Script::new(),
        );
        world.with(|cx| push_theme(cx).expect("a missing picture is not fatal here"));
        assert_eq!(world.signals(), SIGNALS);
    }

    #[test]
    fn an_unmoved_scheme_signals_nothing_at_all() {
        let world = World::new(
            "theme-gate-closed",
            "[appearance]\ntheme_mode = \"dark\"\n",
            Script::new(),
        );
        fs::create_dir_all(&world.paths.generated).expect("scratch");
        fs::write(&world.paths.markers.color_scheme, "dark\n").expect("scratch");
        world.with(|cx| apply_theme_if_scheme_moved(cx).expect("nothing to do"));
        assert!(world.signals().is_empty());
    }

    #[test]
    fn a_moved_scheme_re_themes_and_then_reloads() {
        let world = World::new(
            "theme-gate-open",
            "[appearance]\ntheme_mode = \"light\"\n",
            Script::new(),
        );
        fs::create_dir_all(&world.paths.generated).expect("scratch");
        fs::write(&world.paths.markers.color_scheme, "dark\n").expect("scratch");
        world.with(|cx| apply_theme_if_scheme_moved(cx).expect("the palette moves"));
        let signals = world.signals();
        assert_eq!(
            signals.first().map(String::as_str),
            Some("gsettings set org.gnome.desktop.interface color-scheme prefer-light")
        );
        assert_eq!(signals.last().map(String::as_str), Some("hyprctl reload"));
    }

    #[test]
    fn a_refused_reload_is_reported_in_the_steps_own_words() {
        let world = World::new(
            "theme-reload-refused",
            "[appearance]\ntheme_mode = \"light\"\n",
            Script::new().failing("hyprctl reload"),
        );
        fs::create_dir_all(&world.paths.generated).expect("scratch");
        fs::write(&world.paths.markers.color_scheme, "dark\n").expect("scratch");
        world.with(|cx| {
            let error = apply_theme_if_scheme_moved(cx).expect_err("the reload was refused");
            assert_eq!(error.to_string(), "Unable to reload theme");
        });
    }
}
