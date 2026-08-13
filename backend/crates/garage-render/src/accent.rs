//! `render_accent()`: publish the accent for the shell to read. The file half of the accent
//! route.
//!
//! `Route::Accent`'s only step is an apply -- there is no
//! [`RenderStep`](garage_core::schema::routes::RenderStep) variant for it, because the
//! accent's applier does the writing itself before it pushes: the Python's `apply_accent()`
//! calls `render_accent()` then `push_accent()` in that order, and the Rust port keeps the
//! same shape by having the apply-side stub reach this function directly rather than through
//! [`crate::dispatch`]. This module is still reached from [`crate::all::render_all`], which
//! runs it unconditionally along with every other renderer.
//!
//! Narrow by design: publishing the accent touches no palette. The rest of the theme is
//! derived from the resolved scheme alone, so a render this small must not drag
//! [`crate::theme::render_theme`] in behind it, or picking a new accent colour would rewrite
//! a dozen unrelated toolkit configs for nothing.

use garage_core::fs::marker::write_marker;

use crate::cx::RenderCx;
use crate::error::RenderError;

/// Write the accent marker the shell and `push_accent()` both read: the accent's own name --
/// `"blue"`, `"teal"`, ... -- followed by a newline (garage:2399-2401's `render_accent()`).
///
/// # Errors
///
/// [`RenderError::Marker`] if the marker could not be written.
pub fn render_accent(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let text = format!("{}\n", cx.prefs().appearance.accent_color.as_str());
    write_marker(&cx.paths().markers.accent, &text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::traits::{LuaCheckError, Monitor, MonitorError, MonitorSource};
    use std::collections::HashMap;
    use std::path::Path;

    use super::render_accent;
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
    fn the_default_desktop_writes_the_default_accent() {
        let temp = std::env::temp_dir().join(format!("garage-accent-test-{}", std::process::id()));
        let paths = paths(&temp);
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);

        render_accent(&cx).expect("render_accent writes the marker");

        let written = std::fs::read_to_string(&paths.markers.accent).expect("marker exists");
        assert_eq!(
            written,
            format!("{}\n", defaults.values().appearance.accent_color.as_str())
        );
        drop(std::fs::remove_dir_all(&temp));
    }
}
