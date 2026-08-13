//! `apply_wallpaper()` and `apply_live_wallpaper()`: put the resolved wallpaper on screen.
//!
//! `apply_wallpaper()` re-points the `current` symlink first, atomically, then decides how
//! hyprpaper needs to hear about it. hyprpaper 0.8.4 reads `fit_mode` from its config only at
//! startup and exposes no reload over IPC, so a config change (a fit change, or the first
//! wallpaper on a machine that has never rendered one) has to restart the service; anything
//! else -- the picture changed but the fit did not -- goes over `hyprctl hyprpaper wallpaper`
//! instead, because a restart would blank every monitor for as long as hyprpaper takes to
//! come back. The resolved target matters for that IPC call: hyprpaper keys its cache on the
//! string it was handed, so re-issuing the `current` symlink's own path is a no-op even once
//! the link points somewhere new -- the resolved target is what actually changes the cache
//! key.
//!
//! `apply_live_wallpaper()` is the one-appearance route's applier: it dresses the desktop
//! only if the appearance the changed key belongs to is the one on screen right now, because
//! dressing the light appearance's wallpaper from a dark session must not change what is
//! behind the pane. `wallpaper_fit` belongs to neither half and always lands through
//! `apply_wallpaper()` directly, which is why it has no route of its own here.
//!
//! See [`target`] for the resolver both of them start from.

pub(crate) mod target;

use std::path::Path;

use garage_core::schema::enums::Scheme;
use garage_render::all::render_wallpaper;
use garage_render::theme::resolve_theme;

use crate::command::run;
use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::wallpaper::target::{monitor_names, wallpaper_target};

/// Whether `hyprpaper.conf` moved, when the caller already knows.
///
/// `apply_preferences()` is the one caller that has already rendered the fragment itself --
/// `render_all()` writes it there -- and so has taken the changed/unchanged answer this would
/// otherwise compute. [`Moved::Ask`] is "render it now and see", which is every other caller.
/// The Python spells the same thing as `config_changed: bool | None = None`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Moved {
    /// Render `hyprpaper.conf` here and use its answer.
    Ask,
    /// The caller already rendered it, and this is what it reported.
    Known(bool),
}

/// Put the resolved wallpaper for the currently active appearance on screen
/// (garage:3640-3679).
///
/// # Errors
///
/// Whatever [`wallpaper_target`] refuses -- a missing picture, one that is not an image, or a
/// colour `magick` could not render -- plus [`ApplyError::Io`] if the `current` symlink could
/// not be re-pointed and [`ApplyError::Render`] if `hyprpaper.conf` could not be written.
pub(crate) fn apply_wallpaper(cx: &mut SessionCx<'_>, moved: Moved) -> Result<(), ApplyError> {
    let scheme = resolve_theme(cx.render().prefs());
    let Some(path) = wallpaper_target(cx, scheme)? else {
        return Ok(());
    };
    point_current_at(cx.render().paths().wallpaper.current.as_path(), &path)?;

    let changed = match moved {
        Moved::Ask => render_wallpaper(cx.render())?,
        Moved::Known(flag) => flag,
    };
    if changed {
        drop(run(
            cx,
            &["systemctl", "--user", "restart", "hyprpaper.service"],
        ));
        return Ok(());
    }

    // A restart blanks every monitor for as long as hyprpaper takes to come back, so a
    // picture that changed but kept its fit goes over IPC instead. The resolved target
    // matters: hyprpaper keys its cache on the string it was handed, so re-issuing the
    // `current` symlink is a no-op even once the link points somewhere else.
    let target = std::fs::canonicalize(&path).unwrap_or(path);
    let names = monitor_names(cx);
    // Collected rather than short-circuited: the Python builds the whole list of results and
    // *then* asks `any(...)`, so every monitor is issued its own wallpaper even when the first
    // one refuses. An `any()` here would leave the second screen showing the old picture.
    let results: Vec<bool> = names
        .iter()
        .map(|monitor| {
            let argument = format!("{monitor},{}", target.display());
            run(cx, &["hyprctl", "hyprpaper", "wallpaper", &argument]).status != 0
        })
        .collect();
    let refused = results.contains(&true);
    if names.is_empty() || refused {
        // hyprpaper is not up, or refused the image. Starting it fresh is the only remaining
        // way to get the wallpaper on screen.
        drop(run(
            cx,
            &["systemctl", "--user", "restart", "hyprpaper.service"],
        ));
    }
    Ok(())
}

/// `temporary.symlink_to(path); os.replace(temporary, CURRENT_WALLPAPER)`.
///
/// The rename is what makes it atomic: a reader that opens `current` sees either the old
/// target or the new one, never a link being rewritten under it. The staging name is fixed
/// rather than randomised because the Python's is -- `.current.tmp` beside the link -- and
/// two concurrent applies racing on it would each still leave a complete link behind.
fn point_current_at(current: &Path, path: &Path) -> Result<(), ApplyError> {
    let Some(directory) = current.parent() else {
        return Err(ApplyError::Io(format!(
            "{} has no parent directory",
            current.display()
        )));
    };
    std::fs::create_dir_all(directory).map_err(|error| ApplyError::Io(error.to_string()))?;
    let temporary = directory.join(".current.tmp");
    drop(std::fs::remove_file(&temporary));
    std::os::unix::fs::symlink(path, &temporary)
        .map_err(|error| ApplyError::Io(error.to_string()))?;
    std::fs::rename(&temporary, current).map_err(|error| ApplyError::Io(error.to_string()))
}

/// Dress the desktop only if `scheme` is the appearance currently on screen
/// (garage:4872-4880).
///
/// # Errors
///
/// Whatever [`apply_wallpaper`] refuses, and nothing at all when the appearance is not the
/// one on screen -- that arm never reaches the resolver, so a light-half picture that has
/// been deleted cannot fail a dark session's `set`.
pub(crate) fn apply_live_wallpaper(
    cx: &mut SessionCx<'_>,
    scheme: Scheme,
) -> Result<(), ApplyError> {
    if resolve_theme(cx.render().prefs()) == scheme {
        return apply_wallpaper(cx, Moved::Ask);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use garage_core::schema::enums::Scheme;

    use super::{apply_live_wallpaper, apply_wallpaper, Moved};
    use crate::testing::{Script, World};
    use crate::wallpaper::target::{current_wallpaper, monitor_names, wallpaper_target};

    /// Two monitors of different sizes, so `solid_wallpaper()`'s independent maxima are
    /// visible: the geometry is 3840x1800, which is neither display's own resolution.
    const MONITORS: &str = r#"[{"name": "eDP-1", "width": 2880, "height": 1800},
        {"name": "DP-2", "width": 3840, "height": 2160}]"#;

    fn machine() -> Script {
        Script::new()
            .answering("hyprctl monitors -j", 0, MONITORS, "")
            .answering("file --brief --mime-type -L", 0, "image/png\n", "")
    }

    #[test]
    fn a_colour_source_renders_one_png_named_after_the_swatch_and_reuses_it() {
        let world = World::new(
            "wallpaper-colour",
            "[appearance]\ntheme_mode = \"dark\"\nwallpaper_dark_source = \"color\"\n\
             wallpaper_dark_color = \"#1C1C1E\"\n",
            machine(),
        );
        let expected = world.paths.wallpaper.directory.join("solid-1c1c1e.png");
        world.with(|cx| {
            // `magick` is a no-op shim here, so the file has to be planted for the
            // "exited zero and wrote nothing" guard not to fire.
            fs::create_dir_all(&cx.render().paths().wallpaper.directory).expect("scratch");
            fs::write(&expected, b"png").expect("scratch");
            let target = wallpaper_target(cx, Scheme::Dark).expect("the colour resolves");
            assert_eq!(target.as_deref(), Some(expected.as_path()));
        });
        // Reused: the second call finds the file and never runs magick at all.
        assert!(!world.trace().iter().any(|line| line.starts_with("magick")));
    }

    #[test]
    fn a_missing_picture_is_refused_in_the_pythons_own_words() {
        let world = World::new(
            "wallpaper-missing",
            "[appearance]\ntheme_mode = \"light\"\nwallpaper_light = \"/nope/absent.png\"\n",
            machine(),
        );
        world.with(|cx| {
            let error = wallpaper_target(cx, Scheme::Light).expect_err("the file is absent");
            assert_eq!(
                error.to_string(),
                "Wallpaper does not exist: /nope/absent.png"
            );
        });
    }

    #[test]
    fn a_file_that_is_not_an_image_is_refused_after_the_mime_read() {
        let world = World::new(
            "wallpaper-not-image",
            "[appearance]\ntheme_mode = \"light\"\n",
            Script::new()
                .answering("hyprctl monitors -j", 0, MONITORS, "")
                .answering("file --brief --mime-type -L", 0, "text/plain\n", ""),
        );
        let picture = world.home.join("notes.txt");
        fs::create_dir_all(&world.home).expect("scratch");
        fs::write(&picture, b"hello").expect("scratch");
        // No stored picture for either appearance, so `current` is the fallback -- planted
        // here as the link the Python's `current_wallpaper()` would resolve.
        fs::create_dir_all(&world.paths.wallpaper.directory).expect("scratch");
        std::os::unix::fs::symlink(&picture, &world.paths.wallpaper.current).expect("scratch");
        world.with(|cx| {
            let error = wallpaper_target(cx, Scheme::Light).expect_err("not an image");
            assert_eq!(
                error.to_string(),
                format!("Wallpaper is not an image: {}", picture.display())
            );
        });
    }

    #[test]
    fn an_appearance_with_nothing_to_fall_back_to_resolves_to_nothing() {
        let world = World::new(
            "wallpaper-first-session",
            "[appearance]\ntheme_mode = \"light\"\n",
            machine(),
        );
        world.with(|cx| {
            assert_eq!(current_wallpaper(cx.render().paths()), "");
            assert_eq!(
                wallpaper_target(cx, Scheme::Light).expect("no picture is not an error"),
                None
            );
            // And an apply over it signals nothing at all.
            apply_wallpaper(cx, Moved::Ask).expect("nothing to do");
        });
        assert!(world.signals().is_empty());
    }

    #[test]
    fn an_unchanged_fragment_goes_over_ipc_once_per_monitor() {
        let world = World::new(
            "wallpaper-ipc",
            "[appearance]\ntheme_mode = \"light\"\n",
            machine(),
        );
        let picture = world.home.join("pic.png");
        fs::create_dir_all(&world.home).expect("scratch");
        fs::write(&picture, b"png").expect("scratch");
        fs::create_dir_all(&world.paths.wallpaper.directory).expect("scratch");
        std::os::unix::fs::symlink(&picture, &world.paths.wallpaper.current).expect("scratch");
        world.with(|cx| {
            assert_eq!(monitor_names(cx), ["eDP-1", "DP-2"]);
            apply_wallpaper(cx, Moved::Known(false)).expect("the picture lands");
        });
        let issued: Vec<String> = world
            .trace()
            .into_iter()
            .filter(|line| line.starts_with("hyprctl hyprpaper"))
            .collect();
        assert_eq!(
            issued,
            [
                format!("hyprctl hyprpaper wallpaper eDP-1,{}", picture.display()),
                format!("hyprctl hyprpaper wallpaper DP-2,{}", picture.display()),
            ]
        );
        // The fit did not move, so nothing was restarted.
        assert!(!world
            .trace()
            .iter()
            .any(|line| line.contains("hyprpaper.service")));
    }

    #[test]
    fn a_moved_fragment_restarts_the_service_instead_of_talking_to_it() {
        let world = World::new(
            "wallpaper-restart",
            "[appearance]\ntheme_mode = \"light\"\n",
            machine(),
        );
        let picture = world.home.join("pic.png");
        fs::create_dir_all(&world.home).expect("scratch");
        fs::write(&picture, b"png").expect("scratch");
        fs::create_dir_all(&world.paths.wallpaper.directory).expect("scratch");
        std::os::unix::fs::symlink(&picture, &world.paths.wallpaper.current).expect("scratch");
        world.with(|cx| apply_wallpaper(cx, Moved::Known(true)).expect("the picture lands"));
        assert!(world
            .trace()
            .contains(&"systemctl --user restart hyprpaper.service".to_owned()));
        assert!(!world
            .trace()
            .iter()
            .any(|line| line.starts_with("hyprctl hyprpaper")));
    }

    #[test]
    fn the_wrong_appearance_never_reaches_the_resolver() {
        let world = World::new(
            "wallpaper-other-half",
            "[appearance]\ntheme_mode = \"dark\"\nwallpaper_light = \"/nope/absent.png\"\n",
            machine(),
        );
        world.with(|cx| {
            apply_live_wallpaper(cx, Scheme::Light).expect("the light half is not on screen");
        });
        assert!(world.signals().is_empty());
    }
}
