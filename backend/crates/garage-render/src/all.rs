//! `render_all()`: write every generated fragment from the stored configuration.
//!
//! Pure, and that is the contract rather than a happy accident: this writes files and
//! signals nothing. No `gsettings`, no service restart, no `pkill`, no `hyprctl eval` or
//! `reload`. Everything that moves the running desktop lives on the apply side --
//! `push_accent()`, `push_corner_radius()`, `push_theme()`, `apply_wallpaper()`,
//! `apply_night_shift()` -- and `garage-apply`'s `apply_preferences()` is where the two
//! halves are put back together.
//!
//! # The regression this split exists to prevent
//!
//! `render_all()` used to apply as well, which made every caller an applier whether it wanted
//! to be or not: `hypridle.service`'s `ExecStartPre` ran `garage render`, so restarting the
//! locker to pick up a new lock timeout also re-themed every toolkit, pushed `gsettings` and
//! reloaded the compositor -- and at session start it did all of that a second time, once
//! from that unit and once from the `garage apply` in `autostart.lua`.
//!
//! [`RenderCx`] and `garage-apply`'s `SessionCx` are what make that mistake impossible to
//! reintroduce rather than merely undesirable: a renderer has no field, no dependency edge
//! and no widening operation that would let it reach a [`Runner`](garage_core::traits::Runner)
//! back, so `render_all()` cannot grow a call to one the way it once did. Here that mistake
//! does not compile.
//!
//! `hyprctl monitors` still runs from the workspace renderer, through
//! [`RenderCx::monitors`]'s question rather than an instruction: the plan cannot be written
//! without knowing which displays exist. Nothing here answers back.

use crate::bar::{widgets as bar_widgets, workspaces as bar_workspaces};
use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::keybinds::Document;
use crate::workspaces::plan;
use crate::{
    accent, corner, displays, general, keybinds, motion, preferences, region, theme, wallpaper,
};

/// Write every generated fragment, in the Python's `render_all()` order.
///
/// `render_search_engine()` and `render_idle()` are not called from here, exactly as they are
/// not in the Python: the first is reached from [`theme::render_theme`], the second from
/// [`preferences::render_preferences`], and both are also reachable directly through
/// [`crate::dispatch::run_render`] for the routes that need only one of them.
///
/// The Python calls `render_keybinds(load_keybindings())`, loading the shortcut document
/// here. That load parses `keybindings.toml` and filters it against the published catalog,
/// which is `garage-apply`'s `keybind` module and not reachable from this crate, so the
/// document arrives as an argument instead -- the caller loads it and hands it in. Nothing
/// else about the call moves: it is still the second thing written, from the same document,
/// with the same bytes.
///
/// # Errors
///
/// The first renderer's error, in call order below. Every step is real, including the last:
/// task 3.8 replaced the display fragment's stub, so a `render` completes end to end both
/// when `displays.toml` names displays and when it names none -- see
/// [`render_display_fragment`] for the gate between those two.
///
/// [`wallpaper::render_wallpaper`]'s own answer -- whether `hyprpaper.conf` moved, and so
/// whether the service has to be restarted -- is dropped here exactly as the Python drops
/// it: `render_all()` is not the caller that restarts anything, and
/// `garage-apply`'s `apply_preferences()` is the one that asks for the flag.
pub fn render_all(
    cx: &RenderCx<'_>,
    document: &Document,
    browser: general::BrowserCommand<'_>,
) -> Result<(), RenderError> {
    preferences::render_preferences(cx)?;
    keybinds::render_keybinds(cx, document)?;
    general::render_general(cx, browser)?;
    region::render_region(cx)?;
    plan::render_workspaces(cx)?;
    bar_workspaces::render_bar_workspaces(cx)?;
    bar_widgets::render_bar_widgets(cx)?;
    motion::render_motion(cx)?;
    accent::render_accent(cx)?;
    corner::render_corner_radius(cx)?;
    theme::render_theme(cx)?;
    let _moved = wallpaper::render_wallpaper(cx)?;
    render_display_fragment(cx)
}

/// `garage render-bar`'s whole body: `render_region()`, `render_bar_workspaces()`,
/// `render_bar_widgets()`, in that order (garage:6673-6684).
///
/// waybar's `modules-left`, `modules-right`, `modules-center` and `height` now live only in
/// these generated fragments, so the bar comes up with an empty left side and an empty right
/// side if it starts before the first render -- `waybar.service` runs this as its
/// `ExecStartPre` for exactly that reason. Narrow rather than a full [`render_all`], which
/// would rewrite a dozen toolkit configs the bar does not read; nothing is signalled here
/// either, because the unit starts the bar right after and it reads these fragments on the
/// way up.
///
/// `garage-cli` is this function's only caller outside this crate -- see `garage-render`'s
/// module docs for why `bar` and `region` are otherwise private.
///
/// # Errors
///
/// The first of [`region::render_region`], [`bar_workspaces::render_bar_workspaces`] or
/// [`bar_widgets::render_bar_widgets`] to fail.
pub fn render_bar(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    region::render_region(cx)?;
    bar_workspaces::render_bar_workspaces(cx)?;
    bar_widgets::render_bar_widgets(cx)
}

/// `garage render-wallpaper`'s whole body: `render_wallpaper(load_preferences())`
/// (garage:6689-6694).
///
/// Deliberately not `render`: a full [`render_all`] rewrites a dozen toolkit configs, wasteful
/// to chain onto a second unit at session start. Nothing is signalled here either -- the unit
/// starts hyprpaper next, which reads the file this just wrote. Reported flag included, unlike
/// [`render_all`]'s own call: `garage-cli`'s caller has nothing else to hand it to, so it is
/// the return value here rather than a local dropped in place.
///
/// # Errors
///
/// Whatever [`wallpaper::render_wallpaper`] returns.
pub fn render_wallpaper(cx: &RenderCx<'_>) -> Result<bool, RenderError> {
    wallpaper::render_wallpaper(cx)
}

/// `layout = load_display_config(); if layout["displays"]: render_displays(layout) else:
/// DISPLAY_FRAGMENT.unlink(missing_ok=True)` (garage:3515-3518).
///
/// The gate lives here and not inside [`displays::render_displays`] itself: the Python's own
/// `render_all()` is where the check is written, and `render_displays()` is handed an
/// already-nonempty layout by every caller. A `displays.toml` with no `[[display]]` entries
/// at all -- true of every scratch environment nothing has ever run `display-confirm`
/// against -- takes the `else` branch and completes with nothing further to do; a malformed
/// file (a `display` key that is present but not an array) surfaces as
/// [`RenderError::Toml`], the same refusal `load_display_config()` raises, uncaught by
/// `render_all()` in the Python either.
///
/// The body itself lives in [`displays::render_saved_displays`], beside the renderer it
/// gates, so the load-and-gate exists once and can be tested without standing up a whole
/// render. The call stays here, in the Python's own position: last.
///
/// # Errors
///
/// [`RenderError::Toml`] if `displays.toml` cannot be read, parsed, or shaped as an array
/// under `display`; [`RenderError::Remove`] if an empty layout's stale fragment cannot be
/// taken away; otherwise whatever [`displays::render_displays`] returns.
fn render_display_fragment(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    displays::render_saved_displays(cx)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::{
        LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource,
    };

    use super::render_all;
    use crate::cx::RenderCx;
    use crate::error::RenderError;
    use crate::keybinds::Document;

    struct NoMonitors;
    impl MonitorSource for NoMonitors {
        fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
            Ok(vec![])
        }
    }

    struct LuaAccepts;
    impl LuaSyntaxCheck for LuaAccepts {
        fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
            Ok(())
        }
    }

    fn scratch_paths(label: &str) -> Paths {
        let home = std::env::temp_dir().join(format!(
            "garage-render-all-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        Paths::from_env_map(&env)
    }

    /// The scenario this task's report calls out by name: a fresh scratch tree with no
    /// `displays.toml` at all completes `render_all()` end to end, `render_displays()`'s own
    /// the display fragment's own branch, which takes the fragment away rather than
    /// writing one because `layout["displays"]` is empty.
    #[test]
    fn render_all_completes_with_no_displays_toml_in_scratch() {
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let paths = scratch_paths("no-displays-toml");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);
        assert!(!paths.host.displays.exists());

        render_all(&cx, &Document::default(), None)
            .expect("render_all completes with no displays.toml");
        assert!(!paths.fragments.displays.exists());
        drop(std::fs::remove_dir_all(&paths.home));
    }

    /// `displays.toml` present but empty (`display = []`, or the key absent) takes the same
    /// branch as a missing file, and also removes a stale fragment left over from a layout
    /// that has since been cleared.
    #[test]
    fn render_all_removes_a_stale_display_fragment_when_the_saved_layout_is_empty() {
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let paths = scratch_paths("stale-fragment");
        std::fs::create_dir_all(&paths.root).expect("layer 2 directory is creatable");
        std::fs::write(&paths.host.displays, "display = []\n").expect("displays.toml is writable");
        std::fs::create_dir_all(&paths.generated).expect("generated directory is creatable");
        std::fs::write(&paths.fragments.displays, "-- stale\n")
            .expect("a stale fragment is writable");

        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);
        render_all(&cx, &Document::default(), None)
            .expect("render_all completes with an empty saved layout");
        assert!(!paths.fragments.displays.exists());
        drop(std::fs::remove_dir_all(&paths.home));
    }

    /// A `displays.toml` naming one enabled display now writes the per-monitor fragment
    /// rather than stopping anywhere -- every step of `render_all()` reaches a real body.
    #[test]
    fn render_all_writes_the_display_fragment_once_a_display_is_saved() {
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let paths = scratch_paths("with-a-display");
        std::fs::create_dir_all(&paths.root).expect("layer 2 directory is creatable");
        std::fs::write(
            &paths.host.displays,
            "primary = \"DP-1\"\n\n[[display]]\noutput = \"DP-1\"\nenabled = true\n",
        )
        .expect("displays.toml is writable");

        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);
        render_all(&cx, &Document::default(), None)
            .expect("render_all completes with a saved layout");
        assert_eq!(
            std::fs::read_to_string(&paths.fragments.displays).ok(),
            Some(
                "-- Generated by garage. Edit displays.toml instead.\n\
                 PRIMARY_MONITOR = \"DP-1\"\n\
                 hl.monitor({ output = \"DP-1\", mode = \"preferred\", position = \"0x0\", \
                 scale = \"1\", transform = 0, vrr = -1 })\n"
                    .to_owned()
            )
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }

    /// A `display` key of the wrong shape is the Python's own refusal, uncaught by
    /// `render_all()` there either.
    #[test]
    fn a_display_key_that_is_not_an_array_refuses_the_whole_render() {
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let paths = scratch_paths("display-not-an-array");
        std::fs::create_dir_all(&paths.root).expect("layer 2 directory is creatable");
        std::fs::write(&paths.host.displays, "display = 1\n").expect("displays.toml is writable");

        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);
        let error = render_all(&cx, &Document::default(), None).expect_err("the shape is refused");
        assert!(matches!(error, RenderError::Toml { .. }));
        assert_eq!(
            error.to_string(),
            "displays.toml: display must be an array of tables"
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
