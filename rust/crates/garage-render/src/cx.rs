//! The render context: what a renderer is given, and therefore what it can do.

use std::fmt;

use garage_core::paths::Paths;
use garage_core::schema::Preferences;
use garage_core::traits::{LuaSyntaxCheck, MonitorSource};

/// Everything a renderer is handed, and nothing else.
///
/// A renderer reads layers 1 and 2 and writes layer 3. It writes files and signals
/// nothing: no `gsettings`, no service restart, no `pkill`, no `hyprctl eval` or
/// `reload`. Everything that moves the running desktop lives on the apply side, and
/// `garage_apply`'s `SessionCx` is where the two halves are put back together.
///
/// The Python enforces that by docstring and by review. Here it is enforced by what this
/// struct can name.
///
/// # No lock, and no way to reach one
///
/// Invariant 4: the render path must never be able to take `PREFERENCES_LOCK`. There is
/// no lock field here, which is the visible half. The half that makes it structural is
/// that `garage-render`'s `Cargo.toml` does not list `garage-prefs`, so `PrefLock::acquire`
/// does not resolve from this crate -- a renderer that wanted the lock could not write the
/// line. `tests/test_lint.py`'s crate-graph test reads `cargo metadata` and fails the
/// build if that edge ever appears, transitively included.
///
/// The deadlock it prevents is real and was reachable: `set lock.*` holds
/// `PREFERENCES_LOCK` across a *synchronous* `systemctl --user restart hypridle`, whose
/// `ExecStartPre` runs `garage render-idle`. A render that took the lock would wait for
/// the `set` that is waiting for it. `render_idle()`'s docstring in the Python is the same
/// rule stated the only way Python could state it: "Nothing here takes `PREFERENCES_LOCK`,
/// and nothing here may ever be made to."
///
/// [`prefs`](RenderCx::prefs) is a `&Preferences` -- already loaded, already validated,
/// borrowed. The caller that loaded it is the caller that held the lock, and it is not
/// this one.
///
/// # No process runner, and two named questions
///
/// There is no `&dyn Runner` here either. A general ability to run programs is exactly the
/// ability to move the running session, which is the apply side's job. The two subprocess
/// needs a render genuinely has are named fields rather than a capability:
///
/// * [`monitors`](RenderCx::monitors) is a QUESTION. `render_workspaces()` reaches
///   `hyprctl monitors` through `workspace_outputs()` because the workspace plan cannot be
///   written without knowing which displays exist. It asks and nothing answers back.
/// * [`lua`](RenderCx::lua) is invariant 2's preflight. `write_lua()` runs `luac -p` over
///   a candidate fragment before it is installed, because Hyprland syntax-checks
///   `hyprland.lua` but does not follow its `dofile()`s -- so without the preflight a
///   malformed fragment is discovered at reload, already on disk, and loaded again by
///   every later session.
///
/// Each is a single question with a single answer. Neither can be turned into "run this".
///
/// # The one sanctioned layer-2 write
///
/// Renderers write layer 3 and never layer 2, with one exception, and it is deliberate:
/// `render_workspaces()` reaches `save_workspace_blocks()` by way of `workspace_plan()`
/// and `per_display_groups()`, so a pure `garage render` can write
/// `workspace-blocks.toml`. The allocation has to survive the render that produced it --
/// unremembered, it would be recomputed from the current display ordering, and then
/// unplugging display 2 of 3 slides display 3 into block 2 and drags its windows with it.
/// There is one allocator, and reading it means running it.
///
/// That write does not weaken the lock invariant, which is why this context can carry the
/// path to it ([`Paths::host`]`.workspace_blocks`) without contradiction.
/// `workspace-blocks.toml` is one of the four host files precisely because it is *not*
/// `preferences.toml`: `save_workspace_blocks()` is its single writer, its own
/// single-writer discipline is what serialises it, and `PREFERENCES_LOCK` is not involved
/// on either side. The invariant is about the lock, not about the layer.
///
/// `Copy`, because it is four shared references and nothing else. A caller passes it by
/// value to every renderer it runs without ceremony, and there is nothing in it whose
/// duplication could matter.
#[derive(Clone, Copy)]
pub struct RenderCx<'a> {
    prefs: &'a Preferences,
    paths: &'a Paths,
    monitors: &'a dyn MonitorSource,
    lua: &'a dyn LuaSyntaxCheck,
}

impl<'a> RenderCx<'a> {
    /// Assemble a context from parts the caller already owns.
    ///
    /// Every field is a borrow, so this allocates nothing and owns nothing: a process
    /// loads the preferences once, under whatever lock discipline it answers to, and
    /// lends them to every renderer it runs.
    #[must_use]
    pub const fn new(
        prefs: &'a Preferences,
        paths: &'a Paths,
        monitors: &'a dyn MonitorSource,
        lua: &'a dyn LuaSyntaxCheck,
    ) -> Self {
        Self {
            prefs,
            paths,
            monitors,
            lua,
        }
    }

    /// The effective configuration: layer 1 with layer 2's departures applied, already
    /// validated. Read-only -- a renderer that wanted to change a preference would be an
    /// applier.
    #[must_use]
    pub const fn prefs(&self) -> &'a Preferences {
        self.prefs
    }

    /// Where everything lives. Layer 3's fragments and markers are what a renderer writes;
    /// the host files are what it reads, save for the block allocator's own file.
    #[must_use]
    pub const fn paths(&self) -> &'a Paths {
        self.paths
    }

    /// The compositor, for the one question a render may ask it.
    #[must_use]
    pub const fn monitors(&self) -> &'a dyn MonitorSource {
        self.monitors
    }

    /// The `luac` preflight, run over every generated Lua fragment before it is installed.
    #[must_use]
    pub const fn lua(&self) -> &'a dyn LuaSyntaxCheck {
        self.lua
    }
}

/// Hand-written because two fields are trait objects, which carry no `Debug`. Requiring
/// one from the traits would buy nothing: what is worth printing about a context is where
/// it will write, and that is [`Paths`].
impl fmt::Debug for RenderCx<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RenderCx")
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::{LuaCheckError, Monitor, MonitorError, MonitorSource};

    use super::RenderCx;

    /// A compositor that reports one display. Enough to prove [`MonitorSource`] is
    /// object-safe and that the borrow reaches back out of the context.
    struct OneDisplay;

    impl MonitorSource for OneDisplay {
        fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
            Ok(vec![Monitor {
                name: "DP-1".to_owned(),
                width: 3440,
                height: 1440,
            }])
        }
    }

    /// A machine with no compositor answering.
    struct NoCompositor;

    impl MonitorSource for NoCompositor {
        fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
            Err(MonitorError {
                detail: "no compositor answers here".to_owned(),
            })
        }
    }

    /// A `luac` that likes everything, which is also what a machine without `luac`
    /// behaves like.
    struct LuaAccepts;

    impl garage_core::traits::LuaSyntaxCheck for LuaAccepts {
        fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
            Ok(())
        }
    }

    fn paths() -> Paths {
        let env: HashMap<String, String> = [("HOME".to_owned(), "/home/tester".to_owned())]
            .into_iter()
            .collect();
        Paths::from_env_map(&env)
    }

    #[test]
    fn a_context_hands_back_what_it_was_given() {
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let paths = paths();
        let monitors = OneDisplay;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);

        assert_eq!(cx.paths().home, paths.home);
        assert_eq!(cx.prefs(), defaults.values());
        let seen = cx.monitors().monitors().expect("the fake answers");
        assert_eq!(seen.len(), 1);
        assert_eq!(seen.first().map(|first| first.name.as_str()), Some("DP-1"));
        assert!(cx.lua().check(Path::new("/nonexistent.lua")).is_ok());
    }

    #[test]
    fn a_failing_compositor_reaches_the_caller_as_an_error() {
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let paths = paths();
        let monitors = NoCompositor;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);

        // What `workspace_outputs()` does with it: the saved layout leads, and an
        // unreachable compositor contributes nothing rather than taking the plan down.
        let names: Vec<String> = cx
            .monitors()
            .monitors()
            .unwrap_or_default()
            .into_iter()
            .map(|monitor| monitor.name)
            .collect();
        assert!(names.is_empty());
    }

    #[test]
    fn the_debug_impl_names_the_type_without_the_trait_objects() {
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let paths = paths();
        let monitors = OneDisplay;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);
        assert!(format!("{cx:?}").starts_with("RenderCx"));
    }
}
