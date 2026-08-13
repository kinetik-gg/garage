//! `render_motion()`: publish Reduce Motion for the parts of the desktop Hyprland cannot reach.
//!
//! The compositor has its own switch, and [`crate::lua::emit`]'s `motion_lua()` throws it
//! from inside [`crate::preferences::render_preferences`]. Nothing else does: waybar is a
//! separate process with a stylesheet of its own, and the shell's palettes animate
//! themselves in QML because the entrance they want is not one a Hyprland layer rule can
//! describe. Both read this marker instead of asking the compositor.
//!
//! This is the bar's own Reduce Motion, distinct from the compositor's: waybar cannot see
//! Hyprland's animations switch, so its stylesheet is rewritten and re-read whenever this
//! setting moves -- see `Route::Motion`'s four steps, which render this, render the
//! preferences fragment, then push both the compositor and the bar style.

use garage_core::fs::marker::write_marker;

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the Reduce Motion marker waybar and the shell both watch: `"1\n"` when Reduce
/// Motion is on, `"0\n"` otherwise (garage:2399-2403's `render_motion()`).
///
/// # Errors
///
/// [`RenderError::Marker`] if the marker could not be written.
pub(crate) fn render_motion(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let text = if cx.prefs().appearance.reduce_motion {
        "1\n"
    } else {
        "0\n"
    };
    write_marker(&cx.paths().markers.reduce_motion, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::{LuaCheckError, Monitor, MonitorError, MonitorSource};
    use std::collections::HashMap;
    use std::path::Path;

    use super::render_motion;
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
    fn the_default_desktop_writes_reduce_motion_off() {
        let temp = std::env::temp_dir().join(format!("garage-motion-test-{}", std::process::id()));
        let paths = paths(&temp);
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);

        render_motion(&cx).expect("render_motion writes the marker");

        let written = std::fs::read_to_string(&paths.markers.reduce_motion).expect("marker exists");
        assert_eq!(written, "0\n");
        drop(std::fs::remove_dir_all(&temp));
    }
}
