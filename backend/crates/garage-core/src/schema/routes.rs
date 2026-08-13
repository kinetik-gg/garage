//! `PREFERENCE_ROUTES`: what `set` walks after the file is written.
//!
//! One variant per route, and the calls behind each one in order, as data --
//! the Python's routes are a tuple of function names resolved through
//! `globals()` at call time, so naming them here lets the schema table
//! declare a route, and everything the route does, without this crate being
//! able to run any of it. Resolving a step to a function is the caller's
//! business; saying which steps a route is, and in what order, is the
//! schema's.
//!
//! The split each route preserves: a `render_*` step writes a file and
//! signals nothing, an `apply_*` or `push_*` step moves the running session.

use crate::schema::enums::Scheme;
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

/// One call a route makes, and the only two kinds of call there are.
///
/// The split the Python's table preserves, made a type: a [`Step::Render`]
/// step writes a file and signals nothing, a [`Step::Apply`] step moves the
/// running session. Nothing else can be a step -- a route is these calls in
/// order and no control flow, so anything that is neither a write nor a
/// signal has no way into the table.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// A `render_*` call: writes a fragment, signals nobody.
    Render(RenderStep),
    /// An `apply_*`, `push_*` or `run_or_raise` call: moves the session.
    Apply(ApplyStep),
}

/// The `render_*` half, one variant per renderer the route table names.
///
/// In first-appearance order down `PREFERENCE_ROUTES`, not alphabetical: the
/// table is the reason each of these exists.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RenderStep {
    /// `render_search_engine`: the one fragment the search keys reach.
    SearchEngine,
    /// `render_preferences`: the shared fragment most appearance keys land in.
    Preferences,
    /// `render_motion`: the bar stylesheet's own Reduce Motion.
    Motion,
    /// `render_general`: the launcher marker the wrapper reads on every press.
    General,
    /// `render_idle`: hypridle.conf, and the render the restart re-enters.
    Idle,
}

/// The `apply_*`/`push_*` half, one variant per applier the route table names.
///
/// Same order and same reason as [`RenderStep`]. Two carry arguments, because
/// the Python's step is a tuple there rather than a bare name.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ApplyStep {
    /// `apply_wallpaper`
    Wallpaper,
    /// `("apply_live_wallpaper", "light"|"dark")`: dress one appearance, and
    /// only reach the desktop if it is the one on screen.
    LiveWallpaper(Scheme),
    /// `apply_accent`
    Accent,
    /// `apply_corner_radius`
    CornerRadius,
    /// `apply_border`
    Border,
    /// `apply_motion`
    Motion,
    /// `apply_bar_style`
    BarStyle,
    /// `apply_theme_if_scheme_moved`
    ThemeIfSchemeMoved,
    /// `apply_glass`
    Glass,
    /// `apply_night_shift`
    NightShift,
    /// `apply_terminal`
    Terminal,
    /// `apply_file_index`
    FileIndex,
    /// `("run_or_raise", [command...], "message")`: run the command, and raise
    /// with the message if it fails.
    RunOrRaise {
        /// The argv, verbatim from the Python -- it reaches a real binary.
        command: &'static [&'static str],
        /// What the failure is reported as -- it reaches the user's stderr.
        message: &'static str,
    },
    /// `apply_locale`
    Locale,
    /// `apply_region`
    Region,
    /// `apply_workspace_plan`
    WorkspacePlan,
    /// `apply_bar_workspaces`
    BarWorkspaces,
    /// `apply_bar_widgets`
    BarWidgets,
}

impl Route {
    /// `PREFERENCE_ROUTES`' values: the calls this route makes, in order.
    ///
    /// Transcribed in the Python's table order, with its comments, because
    /// the order is the behaviour: every route that renders does so before it
    /// applies, and the two-step routes that read the file they just wrote
    /// would signal against stale state the other way round.
    #[must_use]
    pub const fn steps(self) -> &'static [Step] {
        use ApplyStep as A;
        use RenderStep as R;
        match self {
            // No theme work on any of these: every palette output is derived
            // from the resolved scheme alone, including the bar foreground.
            // Nothing reads the image, so a new wallpaper cannot change what
            // the theme would emit.
            Self::Wallpaper => &[Step::Apply(A::Wallpaper)],
            Self::WallpaperLight => &[Step::Apply(A::LiveWallpaper(Scheme::Light))],
            Self::WallpaperDark => &[Step::Apply(A::LiveWallpaper(Scheme::Dark))],
            Self::Accent => &[Step::Apply(A::Accent)],
            Self::Search => &[Step::Render(R::SearchEngine)],
            Self::CornerRadius => &[Step::Render(R::Preferences), Step::Apply(A::CornerRadius)],
            Self::Border => &[Step::Render(R::Preferences), Step::Apply(A::Border)],
            // The bar is the third party to this one. Its workspace dots are
            // the only movement outside the compositor, and waybar cannot see
            // Hyprland's animations switch, so the stylesheet carries a Reduce
            // Motion of its own and has to be rewritten and re-read when the
            // setting moves.
            Self::Motion => &[
                Step::Render(R::Preferences),
                Step::Render(R::Motion),
                Step::Apply(A::Motion),
                Step::Apply(A::BarStyle),
            ],
            Self::Theme => &[Step::Apply(A::ThemeIfSchemeMoved)],
            Self::Glass => &[Step::Render(R::Preferences), Step::Apply(A::Glass)],
            Self::NightShift => &[Step::Apply(A::NightShift)],
            Self::Terminal => &[Step::Apply(A::Terminal)],
            // Nothing to reload: the wrapper reads the marker on every press
            // and the shell watches it, so the switch takes effect as it is
            // written.
            Self::Launcher => &[Step::Render(R::General)],
            Self::FileIndex => &[Step::Apply(A::FileIndex)],
            Self::Input => &[
                Step::Render(R::Preferences),
                Step::Apply(A::RunOrRaise {
                    command: &["hyprctl", "reload"],
                    message: "Unable to reload input settings",
                }),
            ],
            // Nothing but hypridle.conf: the whole lock section is timeouts,
            // and no other fragment carries one. Narrow also because the
            // restart is synchronous and its ExecStartPre re-enters this
            // binary as `garage render-idle` -- all of this runs with
            // PREFERENCES_LOCK held by `set`, so the re-entrant render must
            // stay on a path that never takes it.
            Self::Idle => &[
                Step::Render(R::Idle),
                Step::Apply(A::RunOrRaise {
                    command: &["systemctl", "--user", "restart", "hypridle.service"],
                    message: "Unable to reload idle settings",
                }),
            ],
            // Every region key reaches the bar clock -- the locale names its
            // weekday and month, the rest shape the format string -- so all
            // four take the same route out, and the locale takes one step
            // more.
            Self::Locale => &[Step::Apply(A::Locale), Step::Apply(A::Region)],
            Self::Region => &[Step::Apply(A::Region)],
            Self::Workspaces => &[Step::Apply(A::WorkspacePlan)],
            // Nothing in the compositor depends on this, so the desktop is
            // left alone.
            Self::WorkspaceIndicator => &[Step::Apply(A::BarWorkspaces)],
            // The two bar routes. Both end at reload_bar() and neither touches
            // the compositor: the bar is a layer surface with its own config
            // and its own stylesheet, and nothing in Hyprland reads either.
            // They stay apart because one writes stylesheet state while the
            // other writes module-layout state; media spans two module
            // fragments, but still has nothing to do with CSS.
            Self::BarStyle => &[Step::Apply(A::BarStyle)],
            Self::BarWidgets => &[Step::Apply(A::BarWidgets)],
        }
    }
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
    use super::{ApplyStep as A, RenderStep as R, Route, Step};
    use crate::schema::enums::Scheme;
    use crate::schema::prefs::{PreferenceKey, Section};
    use std::collections::{BTreeSet, HashSet};

    /// Every route, in the Python's table order. Not derived from the key
    /// table: the checks below are about the routes themselves, and one of
    /// them is that no key is the only reason a route exists.
    const ROUTES: [Route; 22] = [
        Route::Wallpaper,
        Route::WallpaperLight,
        Route::WallpaperDark,
        Route::Accent,
        Route::Search,
        Route::CornerRadius,
        Route::Border,
        Route::Motion,
        Route::Theme,
        Route::Glass,
        Route::NightShift,
        Route::Terminal,
        Route::Launcher,
        Route::FileIndex,
        Route::Input,
        Route::Idle,
        Route::Locale,
        Route::Region,
        Route::Workspaces,
        Route::WorkspaceIndicator,
        Route::BarStyle,
        Route::BarWidgets,
    ];

    /// The Python function each variant is, so a set of variants can be
    /// compared as a set of names. Both matches are exhaustive on purpose: a
    /// variant added to either enum stops these tests compiling until it is
    /// named here and in the declared list below, which is what makes the
    /// "nothing dead" check a real one rather than a list that drifts.
    fn render_name(step: R) -> &'static str {
        match step {
            R::SearchEngine => "render_search_engine",
            R::Preferences => "render_preferences",
            R::Motion => "render_motion",
            R::General => "render_general",
            R::Idle => "render_idle",
        }
    }

    fn apply_name(step: A) -> &'static str {
        match step {
            A::Wallpaper => "apply_wallpaper",
            A::LiveWallpaper(_) => "apply_live_wallpaper",
            A::Accent => "apply_accent",
            A::CornerRadius => "apply_corner_radius",
            A::Border => "apply_border",
            A::Motion => "apply_motion",
            A::BarStyle => "apply_bar_style",
            A::ThemeIfSchemeMoved => "apply_theme_if_scheme_moved",
            A::Glass => "apply_glass",
            A::NightShift => "apply_night_shift",
            A::Terminal => "apply_terminal",
            A::FileIndex => "apply_file_index",
            A::RunOrRaise { .. } => "run_or_raise",
            A::Locale => "apply_locale",
            A::Region => "apply_region",
            A::WorkspacePlan => "apply_workspace_plan",
            A::BarWorkspaces => "apply_bar_workspaces",
            A::BarWidgets => "apply_bar_widgets",
        }
    }

    /// The declared variants, as the names above: what the table has to use
    /// all of. Whitespace-separated rather than one array element per line
    /// because the list is one list, and it is long.
    const RENDER_DECLARED: &str = "render_search_engine render_preferences render_motion \
         render_general render_idle";
    const APPLY_DECLARED: &str = "apply_wallpaper apply_live_wallpaper apply_accent \
         apply_corner_radius apply_border apply_motion apply_bar_style \
         apply_theme_if_scheme_moved apply_glass apply_night_shift apply_terminal \
         apply_file_index run_or_raise apply_locale apply_region apply_workspace_plan \
         apply_bar_workspaces apply_bar_widgets";

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

    #[test]
    fn every_route_walks_somewhere() {
        let distinct: HashSet<Route> = ROUTES.into_iter().collect();
        assert_eq!(distinct.len(), ROUTES.len(), "a route is listed twice");
        for route in ROUTES {
            assert!(!route.steps().is_empty(), "{route:?} has no steps");
        }
        for key in PreferenceKey::ALL.iter().copied() {
            let Some(route) = key.route() else { continue };
            assert!(distinct.contains(&route), "{key} routes off the table");
        }
    }

    /// The one route whose steps a comment in the Python is about: the
    /// restart is synchronous and re-enters the binary as `garage
    /// render-idle`, so the render has to have happened first and the command
    /// has to be exactly this one.
    #[test]
    fn the_idle_route_renders_then_restarts_hypridle() {
        assert_eq!(
            Route::Idle.steps(),
            &[
                Step::Render(R::Idle),
                Step::Apply(A::RunOrRaise {
                    command: &["systemctl", "--user", "restart", "hypridle.service"],
                    message: "Unable to reload idle settings",
                }),
            ]
        );
    }

    #[test]
    fn the_input_route_renders_then_reloads_the_compositor() {
        assert_eq!(
            Route::Input.steps(),
            &[
                Step::Render(R::Preferences),
                Step::Apply(A::RunOrRaise {
                    command: &["hyprctl", "reload"],
                    message: "Unable to reload input settings",
                }),
            ]
        );
    }

    /// Both renders before both applies, and the bar's style among them:
    /// waybar cannot see Hyprland's animations switch, so its stylesheet is
    /// rewritten and re-read on this route too.
    #[test]
    fn the_motion_route_reaches_the_bar_as_well_as_the_compositor() {
        assert_eq!(
            Route::Motion.steps(),
            &[
                Step::Render(R::Preferences),
                Step::Render(R::Motion),
                Step::Apply(A::Motion),
                Step::Apply(A::BarStyle),
            ]
        );
    }

    /// The locale takes one step more than the other three region keys, and
    /// the launcher takes no apply at all -- the marker is read on every
    /// press, so writing it is the whole of the change.
    #[test]
    fn the_locale_and_launcher_routes_are_the_python_s() {
        assert_eq!(
            Route::Locale.steps(),
            &[Step::Apply(A::Locale), Step::Apply(A::Region)]
        );
        assert_eq!(Route::Region.steps(), &[Step::Apply(A::Region)]);
        assert_eq!(Route::Launcher.steps(), &[Step::Render(R::General)]);
    }

    #[test]
    fn each_live_wallpaper_route_dresses_its_own_appearance() {
        assert_eq!(
            Route::WallpaperLight.steps(),
            &[Step::Apply(A::LiveWallpaper(Scheme::Light))]
        );
        assert_eq!(
            Route::WallpaperDark.steps(),
            &[Step::Apply(A::LiveWallpaper(Scheme::Dark))]
        );
    }

    #[test]
    fn no_step_variant_is_declared_and_unused() {
        let mut rendered = BTreeSet::new();
        let mut applied = BTreeSet::new();
        for route in ROUTES {
            for step in route.steps().iter().copied() {
                match step {
                    Step::Render(render) => rendered.insert(render_name(render)),
                    Step::Apply(apply) => applied.insert(apply_name(apply)),
                };
            }
        }
        assert_eq!(rendered, RENDER_DECLARED.split_whitespace().collect());
        assert_eq!(applied, APPLY_DECLARED.split_whitespace().collect());
    }
}
