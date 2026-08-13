//! The session context: a render context, plus the ability to act on it.

use std::fmt;

use garage_core::traits::Runner;
use garage_render::cx::RenderCx;

/// Everything an applier is handed: a whole [`RenderCx`], and a way to run programs.
///
/// The containment is the design. An applier may render -- `apply_preferences()` in the
/// Python renders first and pushes second, and `render_theme()`/`push_theme()`,
/// `render_accent()`/`push_accent()` are the same pairing one setting at a time. So a
/// [`SessionCx`] *holds* a [`RenderCx`] and hands it out through [`SessionCx::render`],
/// and every renderer that exists is callable from here unchanged.
///
/// A renderer cannot apply, and not by agreement: a [`RenderCx`] has no
/// [`Runner`] field, `garage-render` does not depend on the crate that implements one,
/// and there is no way to widen a `&RenderCx` back into the context that contains it.
/// The relation is one-way by construction, in the direction that is safe.
///
/// That is what stops the drift the Python had to be pulled back from. `render_all()`
/// used to apply as well, which made every caller an applier whether it wanted to be or
/// not: `hypridle.service`'s `ExecStartPre` ran `garage render`, so restarting the locker
/// to pick up a new lock timeout re-themed every toolkit, pushed gsettings and reloaded
/// the compositor -- and at session start it did all of that twice, once from that unit
/// and once from the `garage apply` in `autostart.lua`. Here that mistake does not
/// compile.
///
/// Everything in [`RenderCx`]'s invariants still holds for the render half reached
/// through this one: it is the same context, with the same absent lock and the same two
/// named questions. What this type adds is beside it, not inside it.
pub struct SessionCx<'a> {
    render: RenderCx<'a>,
    proc: &'a dyn Runner,
}

impl<'a> SessionCx<'a> {
    /// Wrap a render context with the ability to move the running session.
    #[must_use]
    pub const fn new(render: RenderCx<'a>, proc: &'a dyn Runner) -> Self {
        Self { render, proc }
    }

    /// The render half, for the renderers an apply runs before it pushes.
    ///
    /// Handing out a `&RenderCx` rather than the fields is what keeps the one-way
    /// relation: a renderer called through this sees exactly what it would see from
    /// `garage render`, and cannot tell that it was called from an applier.
    #[must_use]
    pub const fn render(&self) -> &RenderCx<'a> {
        &self.render
    }

    /// The process boundary. Every command that moves the running desktop goes through
    /// it, so every one of them is in one place to be traced, timed out and tested.
    #[must_use]
    pub const fn proc(&self) -> &'a dyn Runner {
        self.proc
    }
}

/// Hand-written for the same reason [`RenderCx`]'s is: a trait object carries no `Debug`.
impl fmt::Debug for SessionCx<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionCx")
            .field("render", &self.render)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::time::Duration;

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::{
        LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource, Output, RunError,
        Runner, DEFAULT_RUN_TIMEOUT,
    };
    use garage_render::cx::RenderCx;

    use super::SessionCx;

    struct OneDisplay;

    impl MonitorSource for OneDisplay {
        fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
            Ok(vec![Monitor {
                name: "eDP-1".to_owned(),
                width: 1920,
                height: 1080,
            }])
        }
    }

    struct LuaAccepts;

    impl LuaSyntaxCheck for LuaAccepts {
        fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
            Ok(())
        }
    }

    /// Records nothing and runs nothing: the point here is that the three methods are
    /// object-safe and callable through a `&dyn Runner` held in a context.
    struct StubRunner;

    impl Runner for StubRunner {
        fn run(&self, command: &[&str], timeout: Duration) -> Result<Output, RunError> {
            assert_eq!(timeout, DEFAULT_RUN_TIMEOUT);
            Ok(Output {
                status: 0,
                stdout: command.join(" "),
                stderr: String::new(),
            })
        }

        fn spawn_detached(&self, command: &[&str]) -> Result<(), RunError> {
            if command.is_empty() {
                return Err(RunError {
                    detail: "nothing to spawn".to_owned(),
                });
            }
            Ok(())
        }

        fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
            Ok(0)
        }
    }

    fn paths() -> Paths {
        let env: HashMap<String, String> = [("HOME".to_owned(), "/home/tester".to_owned())]
            .into_iter()
            .collect();
        Paths::from_env_map(&env)
    }

    #[test]
    fn an_applier_can_reach_the_render_half_it_contains() {
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let paths = paths();
        let monitors = OneDisplay;
        let lua = LuaAccepts;
        let proc = StubRunner;

        // Built inline, which is the ergonomic claim: one set of borrows serves both
        // contexts and neither has to be kept alive by hand.
        let cx = SessionCx::new(
            RenderCx::new(defaults.values(), &paths, &monitors, &lua),
            &proc,
        );

        assert_eq!(cx.render().paths().state_root, paths.state_root);
        let seen = cx.render().monitors().monitors().expect("the fake answers");
        assert_eq!(seen.first().map(|first| first.name.as_str()), Some("eDP-1"));
    }

    #[test]
    fn the_runner_is_callable_through_the_context() {
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let paths = paths();
        let monitors = OneDisplay;
        let lua = LuaAccepts;
        let proc = StubRunner;
        let cx = SessionCx::new(
            RenderCx::new(defaults.values(), &paths, &monitors, &lua),
            &proc,
        );

        let output = cx
            .proc()
            .run(&["hyprctl", "reload"], DEFAULT_RUN_TIMEOUT)
            .expect("the stub runs");
        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, "hyprctl reload");
        assert!(cx.proc().spawn_detached(&["timedatectl"]).is_ok());
        assert!(cx.proc().spawn_detached(&[]).is_err());
        assert_eq!(
            cx.proc()
                .run_streamed(&["bootstrap.sh"], Some(Path::new("/checkout")))
                .expect("the stub runs"),
            0
        );
    }

    #[test]
    fn a_render_context_outlives_the_session_that_borrowed_it() {
        // The lifetime is the borrows', not the SessionCx's: a reference taken out of a
        // context stays valid after the context is gone, which is what lets a caller
        // build one per operation without threading lifetimes through every renderer.
        let defaults = Defaults::compiled().expect("the shipped defaults parse");
        let paths = paths();
        let monitors = OneDisplay;
        let lua = LuaAccepts;
        let proc = StubRunner;
        let render = {
            let cx = SessionCx::new(
                RenderCx::new(defaults.values(), &paths, &monitors, &lua),
                &proc,
            );
            *cx.render()
        };
        assert_eq!(render.paths().home, paths.home);
    }
}
