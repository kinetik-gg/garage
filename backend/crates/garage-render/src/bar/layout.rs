//! `render_bar_layout()`: publish marker v2 for the shell-native bar.
//!
//! The document contains values and ordered extension ids, never QML definitions. It is
//! written through `write_marker()` so Quickshell's watch remains attached to the same
//! inode; the marker write is the complete apply mechanism.

use garage_core::fs::marker::write_marker;
use garage_core::schema::Preferences;
use garage_core::toml_emit::{json_dumps, Value};

use crate::cx::RenderCx;
use crate::error::RenderError;

const MARKER_VERSION: i64 = 2;

fn widget_ids(text: &str) -> Value {
    Value::Array(
        text.lines()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| Value::Str(id.to_owned()))
            .collect(),
    )
}

fn bar_layout_value(prefs: &Preferences) -> Value {
    let bar = &prefs.bar;
    Value::Table(vec![
        ("version".to_owned(), Value::Int(MARKER_VERSION)),
        (
            "position".to_owned(),
            Value::Str(bar.position.as_str().to_owned()),
        ),
        ("height".to_owned(), Value::Int(bar.height.get())),
        (
            "padding_scale".to_owned(),
            Value::Float(bar.padding_scale.get()),
        ),
        (
            "background".to_owned(),
            Value::Str(bar.background.as_str().to_owned()),
        ),
        (
            "max_group_widgets".to_owned(),
            Value::Int(bar.max_group_widgets.get()),
        ),
        ("left".to_owned(), widget_ids(&bar.widgets_left)),
        ("center".to_owned(), widget_ids(&bar.widgets_center)),
        ("right".to_owned(), widget_ids(&bar.widgets_right)),
    ])
}

/// Write `bar-layout.json` marker v2 in place.
///
/// # Errors
///
/// [`RenderError::Marker`] if the watched marker cannot be written.
pub fn render_bar_layout(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let text = format!("{}\n", json_dumps(&bar_layout_value(cx.prefs()), 2));
    write_marker(&cx.paths().markers.bar_layout, &text)?;
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
    use garage_core::toml_emit::json_dumps;
    use garage_core::traits::{
        LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource,
    };

    use super::{bar_layout_value, render_bar_layout};
    use crate::cx::RenderCx;

    fn prefs_from(departures: &str) -> Preferences {
        let table: toml::Table = departures.parse().expect("fixture toml parses");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        Preferences::coerce_from(&table, &defaults, &mut Notes::new())
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

    fn scratch_paths() -> Paths {
        let home = std::env::temp_dir().join(format!(
            "garage-render-layout-v2-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        Paths::from_env_map(&env)
    }

    #[test]
    fn shipped_defaults_produce_marker_v2() {
        let text = json_dumps(&bar_layout_value(&prefs_from("")), 2);
        assert!(text.contains("\"version\": 2"));
        assert!(text.contains("\"position\": \"top\""));
        assert!(text.contains("\"left\": [\n    \"menu\",\n    \"workspaces\"\n  ]"));
        assert!(text.contains("\"center\": [\n    \"media\"\n  ]"));
        assert!(text.contains("\"max_group_widgets\": 6"));
    }

    #[test]
    fn blank_lines_are_not_extension_ids() {
        let prefs = prefs_from("[bar]\nwidgets_left = \"menu\\n\\nworkspaces\\n\"\n");
        let text = json_dumps(&bar_layout_value(&prefs), 2);
        assert!(!text.contains("\"\""));
    }

    #[test]
    fn marker_is_written_and_reread_identically() {
        let prefs = prefs_from("");
        let paths = scratch_paths();
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);
        render_bar_layout(&cx).expect("marker writes");
        let written = std::fs::read_to_string(&paths.markers.bar_layout).expect("marker reads");
        assert_eq!(
            written,
            format!("{}\n", json_dumps(&bar_layout_value(&prefs), 2))
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
