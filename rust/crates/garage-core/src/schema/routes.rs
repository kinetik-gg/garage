//! `PREFERENCE_ROUTES`' keys: what `set` walks after the file is written.
//!
//! One variant per route, and nothing more. The Python's routes are *data* --
//! a tuple of function names resolved through `globals()` at call time -- and
//! the steps behind each one are the next task's; naming them here first is
//! what lets the schema table declare a route without this crate being able to
//! run anything.
//!
//! The split each route preserves: a `render_*` step writes a file and signals
//! nothing, an `apply_*` or `push_*` step moves the running session.

use crate::schema::prefs::Section;

/// Where one changed key has to reach for the running desktop to be on it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Route {
    /// The shared fit, which always reaches the desktop.
    ///
    /// No theme work on any of the three wallpaper routes: every palette
    /// output is derived from the resolved scheme alone, including the bar
    /// foreground. Nothing reads the image, so a new wallpaper cannot change
    /// what the theme would emit.
    Wallpaper,
    /// The light appearance's picture, source and colour. Only the half that
    /// is on screen may reach the desktop, or dressing the light appearance
    /// from a dark session would change what is behind the pane.
    WallpaperLight,
    /// The dark appearance's half of the same.
    WallpaperDark,
    /// The swatch the whole generated palette is built from.
    Accent,
    /// The search engine and its custom template, which reach one fragment.
    Search,
    CornerRadius,
    Border,
    /// The bar is the third party to this one. Its workspace dots are the only
    /// movement outside the compositor, and waybar cannot see Hyprland's
    /// animations switch, so the stylesheet carries a Reduce Motion of its own
    /// and has to be rewritten and re-read when the setting moves.
    Motion,
    /// The scheme and its schedule; nothing moves unless the resolved scheme
    /// actually changed.
    Theme,
    Glass,
    NightShift,
    Terminal,
    /// Nothing to reload: the wrapper reads the marker on every press and the
    /// shell watches it, so the switch takes effect as it is written.
    Launcher,
    FileIndex,
    /// The whole input section, which reaches the compositor through a reload.
    Input,
    /// Nothing but hypridle.conf: the whole lock section is timeouts, and no
    /// other fragment carries one. Narrow also because the restart is
    /// synchronous and its `ExecStartPre` re-enters the binary as `garage
    /// render-idle` -- all of it under the preferences lock `set` holds, so
    /// the re-entrant render must stay on a path that never takes it.
    Idle,
    /// Every region key reaches the bar clock -- the locale names its weekday
    /// and month, the rest shape the format string -- so all four take the
    /// same route out, and the locale takes one step more.
    Locale,
    /// The other three region keys.
    Region,
    /// The workspace plan, which is the one part of this section the
    /// compositor cares about.
    Workspaces,
    /// Only the bar is affected -- the workspaces, their rules and their keys
    /// are untouched -- so this is the one workspaces key that does not
    /// disturb the compositor.
    WorkspaceIndicator,
    /// The bar's stylesheet. Both bar routes end at the same reload and
    /// neither touches the compositor: the bar is a layer surface with its own
    /// config and its own stylesheet, and nothing in Hyprland reads either.
    /// They stay apart because one writes stylesheet state while the other
    /// writes module-layout state.
    BarStyle,
    /// The bar's module list. Media spans two module fragments, but still has
    /// nothing to do with CSS.
    BarWidgets,
}

impl Section {
    /// `SECTION_ROUTES`: the route a key the table does not have would take.
    ///
    /// Unreachable through `set`, which gates on the schema -- so this is not
    /// a fallback the product uses, it is the behaviour a key outside the
    /// schema has always had, kept because a section-wide reload was never an
    /// error and a refactor must not invent one.
    ///
    /// `appearance`, `general` and `bar` are `None` on purpose: each names the
    /// key instead, because each *can* -- every one of their keys routes
    /// somewhere of its own, so there is no section-wide fallback to be
    /// reached and an unknown name really is unknown. `indexing` is absent
    /// from the Python's table too, and an unknown key there has always been
    /// refused by section rather than by name.
    #[must_use]
    pub const fn route(self) -> Option<Route> {
        match self {
            Self::Input => Some(Route::Input),
            Self::Lock => Some(Route::Idle),
            Self::Region => Some(Route::Region),
            Self::Workspaces => Some(Route::Workspaces),
            Self::Appearance | Self::Bar | Self::General | Self::Indexing => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Route;
    use crate::schema::prefs::{PreferenceKey, Section};

    #[test]
    fn every_settable_key_has_a_route() {
        for key in PreferenceKey::ALL.iter().copied() {
            assert!(key.route().is_some(), "{key} has no route");
        }
    }

    #[test]
    fn section_routes_are_the_four_the_python_names() {
        let routed: Vec<Section> = Section::ALL
            .iter()
            .copied()
            .filter(|section| section.route().is_some())
            .collect();
        assert_eq!(
            routed,
            [
                Section::Input,
                Section::Lock,
                Section::Workspaces,
                Section::Region
            ]
        );
        assert_eq!(Section::Lock.route(), Some(Route::Idle));
    }
}
