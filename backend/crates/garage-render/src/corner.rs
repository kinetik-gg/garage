//! `render_corner_radius()`: publish the corner radius for the shell to read. The file half.
//!
//! The rounding in px every rounded surface on the desktop is sized from: a window takes
//! Hyprland's own `decoration:rounding`, Kinetik Glass rounds the layers it decorates from
//! its own plugin options, hyprexpo rounds its overview tiles, and the shell draws its own
//! silhouettes in QML. All four are separate code paths that have to be handed the same
//! number -- see [`crate::lua::emit`]'s `corner_rounding()`, which every one of them reads
//! through -- or the corners visibly disagree with each other.
//!
//! Quickshell watches this marker, so its surfaces re-round with no restart. The world half,
//! `push_corner_radius()`, pushes the same number into the running compositor with `hyprctl
//! eval` and lives on the apply side; this half only ever writes the file.

use garage_core::fs::marker::write_marker;

use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::lua::emit::corner_rounding;

/// Write the corner radius marker the shell reads (`render_corner_radius()`, garage:4750-4753).
///
/// # Errors
///
/// [`RenderError::Marker`] if the marker could not be written.
pub(crate) fn render_corner_radius(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let rounding = corner_rounding(cx.prefs());
    write_marker(&cx.paths().markers.corner_radius, &format!("{rounding}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::schema::notes::Notes;
    use garage_core::schema::Preferences;
    use garage_core::traits::{
        LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource,
    };

    use super::render_corner_radius;
    use crate::cx::RenderCx;

    fn prefs_from(departures: &str) -> Preferences {
        let table: toml::Table = departures.parse().expect("fixture toml parses");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let mut notes = Notes::new();
        Preferences::coerce_from(&table, &defaults, &mut notes)
    }

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
            "garage-render-corner-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        Paths::from_env_map(&env)
    }

    /// `corner_radius`'s three shipped values -- `none`/`normal`/`large` -- against the
    /// `CORNER_RADII` table `corner_rounding()` itself already pins in `lua::emit`'s own
    /// tests; this only checks the marker is the same number followed by a newline.
    #[test]
    fn writes_the_resolved_rounding_followed_by_a_newline() {
        for (setting, expected) in [("none", "0"), ("normal", "7"), ("large", "14")] {
            let prefs = prefs_from(&format!("[appearance]\ncorner_radius = \"{setting}\"\n"));
            let paths = scratch_paths(setting);
            let monitors = NoMonitors;
            let lua = LuaAccepts;
            let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);
            render_corner_radius(&cx).expect("render_corner_radius succeeds on a clean scratch");
            let marker = std::fs::read_to_string(&paths.markers.corner_radius)
                .expect("the corner radius marker was written");
            assert_eq!(marker, format!("{expected}\n"), "corner_radius = {setting}");
            drop(std::fs::remove_dir_all(&paths.home));
        }
    }
}
