//! `apply_bar_workspaces()`, `apply_bar_style()` and `apply_bar_widgets()`: the bar's three
//! fragments, each paired with the one signal that reloads all of them.
//!
//! All three end at `reload_bar()` -- `pkill -USR2 -x waybar`, waybar's default SIGUSR2
//! action, which re-reads the config and every file it includes -- and none of them touch the
//! compositor: the bar is a layer surface with its own config and its own stylesheet, and
//! nothing in Hyprland reads either. They stay three separate appliers because they write
//! three separate fragments: the workspaces indicator does not depend on style, style does
//! not depend on widgets, and media's definition is widget-owned while its place immediately
//! after the workspace indicator is left-side-owned -- so a media toggle alone has to
//! republish both fragments before the one reload.
//!
//! `apply_bar_style()`'s own render half, `render_bar_style()`, is narrow on purpose, the
//! same way `garage render-bar` is: `bar.padding_scale` is a slider and the pane fires a
//! `set` per notch, so going through [`garage_render::theme::render_theme`] instead would
//! rewrite twenty toolkit configs per notch, for a change none of them can see. Only this one
//! file carries `[bar]`'s own styling, built from [`garage_render`]'s `waybar_style_css()`.
//! It writes with `write_marker()` rather than an atomic rename, for the reason every path
//! under `~/.config` waybar watches does: a rename past its `inotify` watch is a change it
//! never hears about.

use garage_core::fs::marker::write_marker;
use garage_render::theme::resolve_theme;
use garage_render::{render_bar_widgets, render_bar_workspaces, waybar_style_css};

use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::workspaces::reload_bar;

/// Publish the bar's module list (menu, workspaces, media) and signal the bar to re-read it
/// (garage:4581-4590).
///
/// # Errors
///
/// [`ApplyError::Render`] if the fragment could not be written.
pub(crate) fn apply_bar_workspaces(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_bar_workspaces(cx.render())?;
    garage_render::render_bar_layout(cx.render())?;
    reload_bar(cx);
    Ok(())
}

/// Rewrite the bar's stylesheet alone, for the resolved appearance (garage:4600-4606).
///
/// `write_marker()`, not an atomic rename, for the reason every path under `~/.config` waybar
/// watches uses it: a rename past its `inotify` watch is a change it never hears about.
///
/// # Errors
///
/// [`ApplyError::Marker`] if the stylesheet could not be written in place.
fn render_bar_style(cx: &SessionCx<'_>) -> Result<(), ApplyError> {
    let prefs = cx.render().prefs();
    let css = waybar_style_css(resolve_theme(prefs), prefs);
    write_marker(&cx.render().paths().toolkit.waybar_style, &css)?;
    Ok(())
}

/// Rewrite the bar's stylesheet alone and have the bar re-read it (garage:4609-4612).
///
/// # Errors
///
/// Whatever [`render_bar_style`] returns.
pub(crate) fn apply_bar_style(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_bar_style(cx)?;
    garage_render::render_bar_layout(cx.render())?;
    reload_bar(cx);
    Ok(())
}

/// Republish the bar's widgets and have the bar re-read them (garage:4615-4625).
///
/// Both fragments, not just the widget one: media's definition is widget-owned while its
/// place immediately after the workspace indicator is left-side-owned, so a media toggle has
/// to republish both before the one reload. The other widget toggles harmlessly reproduce the
/// unchanged left fragment. The Quickshell layout marker is refreshed alongside, so both bar
/// consumers see the same change.
///
/// # Errors
///
/// [`ApplyError::Render`] if either fragment could not be written.
pub(crate) fn apply_bar_widgets(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    render_bar_workspaces(cx.render())?;
    render_bar_widgets(cx.render())?;
    garage_render::render_bar_layout(cx.render())?;
    reload_bar(cx);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_bar_style, apply_bar_widgets, apply_bar_workspaces};
    use crate::testing::{Script, World};

    #[test]
    fn all_three_end_at_the_one_signal_and_never_touch_the_compositor() {
        for (label, applier) in [
            (
                "bar-workspaces",
                apply_bar_workspaces as fn(&mut crate::cx::SessionCx<'_>) -> _,
            ),
            ("bar-style", apply_bar_style),
            ("bar-widgets", apply_bar_widgets),
        ] {
            let world = World::plain(label, Script::new());
            world.with(|cx| applier(cx).expect("the fragment is written"));
            assert_eq!(world.signals(), ["pkill -USR2 -x waybar"], "{label}");
        }
    }

    #[test]
    fn the_stylesheet_is_written_in_place_where_waybar_is_watching() {
        let world = World::plain("bar-style-file", Script::new());
        world.with(|cx| apply_bar_style(cx).expect("the stylesheet is written"));
        let css = std::fs::read_to_string(&world.paths.toolkit.waybar_style)
            .expect("the stylesheet exists");
        assert!(css.contains("@define-color"), "{css}");
    }
}
