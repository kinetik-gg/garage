//! `render_idle()`: `hypridle.conf` from the lock preferences, and nothing else.
//!
//! Its own function because it is its own entry point: `garage render-idle` is
//! `hypridle.service`'s `ExecStartPre`, and hypridle reads exactly this one file -- the
//! generated config names no include and sources nothing, so there is nothing else that unit
//! could need. Narrow on purpose. It used to run the full render there, which meant
//! restarting hypridle to pick up a new lock timeout also re-themed every toolkit, reloaded
//! the compositor and pushed `gsettings`, from inside the `set lock.*` that asked for the
//! restart.
//!
//! Not Lua, and not [`crate::lua`]: `hypridle.conf` is hypridle's own config syntax, and this
//! writes it through [`garage_core::fs::atomic::atomic_write`] -- a whole-file rewrite,
//! same as `preferences.toml` -- rather than through [`garage_core::fs::lua::write_lua`],
//! which is only for fragments `hyprland.lua` `dofile()`s.
//!
//! # The invariant this crate exists to hold
//!
//! Nothing here takes `PREFERENCES_LOCK`, and nothing here may ever be made to. `set lock.*`
//! holds that lock across a *synchronous* `systemctl restart hypridle.service`, whose
//! `ExecStartPre` re-enters this binary and runs this function: waiting on the lock here
//! would deadlock the setting against itself. [`crate::cx::RenderCx`]'s doc names this exact
//! call chain as invariant 4 -- the reason `garage-render` does not depend on `garage-prefs`
//! at all, so the lock this paragraph warns about cannot be reached from this function even
//! by accident.

//! # Four templates, and the one thing that is not one
//!
//! The file is `hypridle-base.tmpl` followed by up to three
//! `hypridle-listener-*.tmpl`s. Which listeners are written is a condition on three
//! timeouts, and a condition decides something -- so it stays here, in Rust, where the
//! schema types are. What each listener *says* decides nothing, so it is a file.
//!
//! Every listener block is spliced between the base and the file's final newline rather
//! than concatenated at the end, which is why they are read with
//! [`Template::expand_block`]: a fragment file ends with the newline every text file ends
//! with, and the block it holds does not.

use garage_core::fs::atomic::atomic_write;

use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::template::shipped::{
    HYPRIDLE_BASE, HYPRIDLE_LISTENER_DPMS, HYPRIDLE_LISTENER_LOCK, HYPRIDLE_LISTENER_SUSPEND,
};
use crate::template::vars::template_vars;
use crate::template::{NoVars, Shipped, Template};

template_vars!(
    /// A listener's own timeout, in seconds. The only thing any of the three listener
    /// fragments takes: what each one does on that timeout is text, and lives in its file.
    IdleListenerVars { timeout: i64 }
);

/// One listener block, from its own fragment.
fn listener(cx: &RenderCx<'_>, shipped: Shipped, timeout: i64) -> Result<String, RenderError> {
    let block = Template::load(cx.paths(), shipped).expand_block(&IdleListenerVars { timeout })?;
    Ok(block)
}

/// Write `hypridle.conf` from `[lock]` alone.
///
/// `lock_timeout`, `display_off_timeout` and `suspend_timeout` each become a listener only
/// when they are past zero -- zero means never, and hypridle would rather be told so than
/// be handed a listener with no timeout worth having. Every configured listener is joined
/// by a blank line, matching the Python's `"\n\n".join(listeners)` exactly, including the
/// case where none are configured at all (an empty join contributes nothing).
///
/// # Errors
///
/// [`RenderError::Template`] if a template on disk names a variable this renderer does not
/// supply, or [`RenderError::Atomic`] if `hypridle.conf` could not be written.
pub(crate) fn render_idle(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let lock = &cx.prefs().lock;
    let mut listeners = Vec::new();
    for (shipped, timeout) in [
        (HYPRIDLE_LISTENER_LOCK, lock.lock_timeout.get()),
        (HYPRIDLE_LISTENER_DPMS, lock.display_off_timeout.get()),
        (HYPRIDLE_LISTENER_SUSPEND, lock.suspend_timeout.get()),
    ] {
        if timeout > 0 {
            listeners.push(listener(cx, shipped, timeout)?);
        }
    }
    let mut idle = Template::load(cx.paths(), HYPRIDLE_BASE).expand(&NoVars)?;
    idle.push_str(&listeners.join("\n\n"));
    idle.push('\n');
    atomic_write(&cx.paths().fragments.hypridle, &idle)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::{LuaCheckError, Monitor, MonitorError, MonitorSource};
    use std::collections::HashMap;
    use std::path::Path;

    use super::render_idle;
    use crate::cx::RenderCx;

    struct NoMonitors;
    impl MonitorSource for NoMonitors {
        fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
            Ok(vec![])
        }
    }

    struct LuaAccepts;
    impl garage_core::traits::LuaSyntaxCheck for LuaAccepts {
        fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
            Ok(())
        }
    }

    fn paths(home: &Path) -> Paths {
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        Paths::from_env_map(&env)
    }

    #[test]
    fn the_default_desktop_writes_the_lock_and_display_off_listeners_but_not_suspend() {
        // Shipped defaults: lock_timeout = 600, display_off_timeout = 900, suspend_timeout
        // = 0 -- suspend is the one left off by default.
        let temp = std::env::temp_dir().join(format!("garage-idle-test-{}", std::process::id()));
        let paths = paths(&temp);
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);

        render_idle(&cx).expect("render_idle writes hypridle.conf");

        let written =
            std::fs::read_to_string(&paths.fragments.hypridle).expect("hypridle.conf exists");
        assert!(written.starts_with("# Generated by garage. Edit preferences.toml instead.\n"));
        assert!(written.contains("timeout = 600\n    on-timeout = loginctl lock-session"));
        assert!(written.contains("timeout = 900\n    on-timeout = hyprctl dispatch"));
        assert!(!written.contains("systemctl suspend"));
        drop(std::fs::remove_dir_all(&temp));
    }

    /// Every shipped template, copied into a scratch world's `templates/` directory the
    /// way stow puts them in `~/.config/garage/templates`.
    fn install_templates(paths: &Paths) {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../desktop/.config/garage/templates");
        let target = paths.root.join("templates");
        std::fs::create_dir_all(&target).expect("the scratch templates directory is creatable");
        for entry in std::fs::read_dir(&source).expect("the shipped templates directory exists") {
            let entry = entry.expect("a directory entry is readable");
            std::fs::copy(entry.path(), target.join(entry.file_name())).expect("a template copies");
        }
    }

    fn render_to(paths: &Paths) -> String {
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), paths, &monitors, &lua);
        render_idle(&cx).expect("render_idle writes hypridle.conf");
        std::fs::read_to_string(&paths.fragments.hypridle).expect("hypridle.conf exists")
    }

    fn scratch(label: &str) -> Paths {
        paths(&std::env::temp_dir().join(format!(
            "garage-idle-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        )))
    }

    /// The templates on disk and the copies compiled in produce the same file.
    ///
    /// The sync check `preferences.defaults.toml` gets, at the level that matters here:
    /// not that the bytes of one file match another, which `include_str!` already makes
    /// true, but that a session reading its stowed templates and a session that has lost
    /// them render the same desktop.
    #[test]
    fn the_stowed_templates_and_the_compiled_copies_render_the_same_file() {
        let stowed = scratch("stowed");
        install_templates(&stowed);
        let compiled = scratch("compiled");
        assert!(!compiled.root.join("templates").exists());

        assert_eq!(render_to(&stowed), render_to(&compiled));
        drop(std::fs::remove_dir_all(&stowed.home));
        drop(std::fs::remove_dir_all(&compiled.home));
    }

    /// Scenario C: the format of a mechanical emitter changes by editing a template, with
    /// no rebuild.
    ///
    /// The binary under this test is the one the other tests in this file run; the only
    /// thing that differs is a file in the scratch world's `templates/` directory. If the
    /// edit reaches the output, then the text of `hypridle.conf` is data on disk rather
    /// than a string literal that needs a compiler to change.
    #[test]
    fn an_edited_template_reaches_the_output_without_a_rebuild() {
        let paths = scratch("edited");
        install_templates(&paths);
        let edited = paths
            .root
            .join("templates")
            .join("hypridle-listener-lock.tmpl");
        std::fs::write(
            &edited,
            "listener {\n    timeout = {{timeout}}\n    on-timeout = loginctl lock-session\n    \
             on-resume = notify-send 'welcome back'\n}\n",
        )
        .expect("the edited template is writable");

        let written = render_to(&paths);

        assert!(written.contains("on-resume = notify-send 'welcome back'"));
        assert!(written.contains("timeout = 600\n    on-timeout = loginctl lock-session"));
        // The listener that was not edited is untouched, so the edit reached exactly the
        // template it was made to.
        assert!(written.contains("timeout = 900\n    on-timeout = hyprctl dispatch"));
        drop(std::fs::remove_dir_all(&paths.home));
    }

    /// A template that is present and names a variable nothing supplies fails the render,
    /// rather than falling back to the compiled copy: an absent template is a machine
    /// missing its dotfiles, a broken one is an edit someone made.
    #[test]
    fn a_template_naming_an_unknown_variable_fails_the_render() {
        let paths = scratch("unknown");
        install_templates(&paths);
        std::fs::write(
            paths
                .root
                .join("templates")
                .join("hypridle-listener-lock.tmpl"),
            "listener {\n    timeout = {{timeuot}}\n}\n",
        )
        .expect("the broken template is writable");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);

        let error = render_idle(&cx).expect_err("a broken template fails the render");

        let message = error.to_string();
        assert!(message.contains("hypridle-listener-lock.tmpl"), "{message}");
        assert!(message.contains("{{timeuot}}"), "{message}");
        drop(std::fs::remove_dir_all(&paths.home));
    }

    // The all-zero (no listener) case, and every other lock timeout combination, is covered
    // by this task's byte-parity fixtures against the real Python backend (see
    // testdata/render_fixtures.json and the differential `render_idle` family), which is a
    // stronger check than a hand-built `Preferences` here -- and building one without a
    // `toml` dev-dependency this crate does not otherwise need is more machinery than a
    // second in-process case is worth.
}
