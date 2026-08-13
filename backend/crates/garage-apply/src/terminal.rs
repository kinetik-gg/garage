//! `apply_terminal()`: push the chosen terminal into the running session.
//!
//! Renders the general markers first (launcher, terminal, browser), then seeds
//! `TERMINAL=` into the systemd user manager's environment -- uwsm starts applications as
//! systemd user units, so this is what makes `$TERMINAL` true for anything launched after the
//! change, though not for anything already running. Reloads the compositor last, because
//! `binds.lua` reads the terminal marker while the config is parsed rather than per keypress,
//! unlike the launcher marker which is read fresh on every press.

use garage_render::render_general;
use garage_render::RenderError;

use crate::command::run;
use crate::cx::SessionCx;
use crate::desktopfiles::roles::{browser_command, terminal_command};
use crate::error::ApplyError;

/// `render_general()` (garage:4446-4459) in full: all three markers, launcher then terminal
/// then browser.
///
/// # Why the third marker is written here and not in `garage-render`
///
/// The Python writes all three from one function, and every caller of `render_general()` --
/// `render_all()` and `apply_terminal()` alike -- gets all three. The port cannot: resolving
/// the browser association runs `env LC_ALL=C gio mime <type>` three times, through
/// `role_applications()` -> `mime_handlers()`, and a
/// [`RenderCx`](garage_render::cx::RenderCx) structurally carries no
/// [`Runner`](garage_core::traits::Runner) and cannot grow one. So
/// [`garage_render::render_general`] writes the two pure markers, and the third is written
/// from here -- on the apply side, which does hold a runner -- at exactly the point the
/// Python writes it: after the other two, from the same resolved-browser lookup, with the
/// same trailing newline.
///
/// Every path with a session behind it goes through here: `apply_terminal()`, the route
/// walk's own `RenderStep::General` arm, `apply_preferences()`, `garage update`'s render step
/// and `garage render` itself, which builds a session context for exactly this one argument.
///
/// # Errors
///
/// [`RenderError::Marker`] if any of the three markers could not be written.
pub(crate) fn publish_general(cx: &SessionCx<'_>) -> Result<(), RenderError> {
    render_general(cx.render(), Some(&resolve_browser(cx)))
}

/// The resolver [`garage_render::render_general`] is handed on every path that has a session
/// behind it: `browser_command()`, called at the moment the marker is written and not before,
/// so the three `gio mime` lookups land where the Python's do.
pub fn resolve_browser<'a>(cx: &'a SessionCx<'a>) -> impl Fn() -> String + 'a {
    move || browser_command(cx)
}

/// Push the resolved terminal choice into the running session (garage:4460-4466).
///
/// # Errors
///
/// Whatever [`publish_general`] returns. The two signals report nothing, as the Python's two
/// bare `run()` calls do.
pub(crate) fn apply_terminal(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    publish_general(cx)?;
    // uwsm starts applications as systemd user units, so this is what makes $TERMINAL true
    // for anything launched after the change.
    let command = terminal_command(
        cx.render().paths(),
        cx.render().prefs().general.terminal.as_str(),
    );
    drop(run(
        cx,
        &[
            "systemctl",
            "--user",
            "set-environment",
            &format!("TERMINAL={command}"),
        ],
    ));
    // binds.lua reads the marker while the config is parsed, not per keypress.
    drop(run(cx, &["hyprctl", "reload"]));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::apply_terminal;
    use crate::testing::{Script, World};

    /// The `gio mime` answers for the browser role's three mimetypes, plus a desktop file
    /// for the handler they name, so `browser_command()` has an `Exec=` line to resolve.
    fn machine() -> Script {
        let answer = "Default application for \u{201c}x\u{201d}: firefox.desktop\n\
                      Registered applications:\n\tfirefox.desktop\n\
                      Recommended applications:\n\tfirefox.desktop\n";
        Script::new()
            .answering("env LC_ALL=C gio mime x-scheme-handler/http", 0, answer, "")
            .answering(
                "env LC_ALL=C gio mime x-scheme-handler/https",
                0,
                answer,
                "",
            )
            .answering("env LC_ALL=C gio mime text/html", 0, answer, "")
    }

    #[test]
    fn all_three_markers_are_published_and_the_browser_one_is_resolved_live() {
        let world = World::plain("apply-terminal", machine());
        let applications = world.home.join(".local/share/applications");
        std::fs::create_dir_all(&applications).expect("scratch");
        std::fs::write(
            applications.join("firefox.desktop"),
            "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox %u\n",
        )
        .expect("scratch");
        world.with(|cx| apply_terminal(cx).expect("the terminal is pushed"));
        assert_eq!(
            std::fs::read_to_string(&world.paths.markers.browser).expect("browser marker"),
            "firefox\n"
        );
        assert!(world.paths.markers.launcher.exists());
        assert!(world.paths.markers.terminal.exists());
        // The three lookups happen on the apply path, which is the whole point of this file.
        assert_eq!(
            world
                .trace()
                .iter()
                .filter(|line| line.starts_with("env LC_ALL=C gio mime"))
                .count(),
            3
        );
        assert_eq!(
            world.signals().last().map(String::as_str),
            Some("hyprctl reload")
        );
    }
}
