//! `render_bar_layout()`: the bar's whole layout state, as one watched marker.
//!
//! The Quickshell bar reads its shape -- height, spacing scale, background, and which
//! widgets are switched on -- from this single file instead of from generated fragments:
//! [`crate::fs::marker::write_marker`] keeps the inode, so the shell's `FileView` watch
//! fires and the bar re-lays itself out live. The write *is* the apply; nothing is
//! signalled and nothing is reloaded.
//!
//! This deliberately carries *values*, not module definitions: what a widget's contents
//! look like is the shell's business, and what the values are is the schema's. One flat
//! document per consumer concern also means one writer per concern -- this file owns
//! every key the `[bar]` section decides plus `workspaces.indicator`, exactly the set the
//! old `BarStyle`/`BarWidgets`/`WorkspaceIndicator` routes moved, so a key cannot land in
//! a fragment its route does not republish.
//!
//! Written alongside the waybar fragments until the waybar cut-over removes them; after
//! that this is the bar's only rendered state beside the clock format marker
//! (`render_region()`'s).

use garage_core::fs::marker::write_marker;
use garage_core::schema::Preferences;
use garage_core::toml_emit::{json_dumps, Value};

use crate::cx::RenderCx;
use crate::error::RenderError;

/// The metric strips the bar can carry, in display order. Mirrors
/// [`crate::bar::widgets`]'s own list; the two must agree because both decide what
/// `monitors` may name.
pub(crate) const BAR_METRICS: &[&str] = &["cpu", "memory", "network", "temp", "disk", "gpu"];

/// Whether `bar.monitor_<name>` is on, for one of [`BAR_METRICS`]' names.
fn monitor_enabled(bar: &garage_core::schema::prefs::Bar, name: &str) -> bool {
    match name {
        "cpu" => bar.monitor_cpu,
        "memory" => bar.monitor_memory,
        "network" => bar.monitor_network,
        "temp" => bar.monitor_temp,
        "disk" => bar.monitor_disk,
        "gpu" => bar.monitor_gpu,
        // Unreachable from BAR_METRICS itself; a name outside that fixed list carries no
        // preference and is treated as switched off rather than panicking.
        _ => false,
    }
}

/// The marker's whole value, in key order. Split from the writer so the shape can be
/// asserted without writing a file.
fn bar_layout_value(prefs: &Preferences) -> Value {
    let bar = &prefs.bar;
    let monitors = BAR_METRICS
        .iter()
        .map(|&name| (name.to_owned(), Value::Bool(monitor_enabled(bar, name))))
        .collect::<Vec<_>>();
    Value::Table(vec![
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
            "indicator".to_owned(),
            Value::Bool(prefs.workspaces.indicator),
        ),
        ("media_player".to_owned(), Value::Bool(bar.media_player)),
        ("ai_usage".to_owned(), Value::Bool(bar.ai_usage)),
        ("monitors".to_owned(), Value::Table(monitors)),
    ])
}

/// Write the bar's layout marker (`bar-layout.json`).
///
/// # Errors
///
/// [`RenderError::Marker`] if the marker could not be written in place.
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
            "garage-render-bar-layout-{label}-{}-{:?}",
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
    fn the_shipped_defaults_produce_the_pinned_document() {
        let prefs = prefs_from("");
        assert_eq!(
            format!("{}\n", json_dumps(&bar_layout_value(&prefs), 2)),
            concat!(
                "{\n",
                "  \"height\": 43,\n",
                "  \"padding_scale\": 1.2,\n",
                "  \"background\": \"transparent\",\n",
                "  \"indicator\": true,\n",
                "  \"media_player\": true,\n",
                "  \"ai_usage\": true,\n",
                "  \"monitors\": {\n",
                "    \"cpu\": true,\n",
                "    \"memory\": true,\n",
                "    \"network\": true,\n",
                "    \"temp\": false,\n",
                "    \"disk\": false,\n",
                "    \"gpu\": false\n",
                "  }\n",
                "}\n"
            )
        );
    }

    #[test]
    fn every_key_the_bar_section_decides_is_carried() {
        let prefs = prefs_from(
            "[bar]\nheight = 55\npadding_scale = 1.75\nbackground = \"blurred\"\n\
             media_player = false\nai_usage = false\nmonitor_temp = true\n\
             [workspaces]\nindicator = false\n",
        );
        assert_eq!(
            format!("{}\n", json_dumps(&bar_layout_value(&prefs), 2)),
            concat!(
                "{\n",
                "  \"height\": 55,\n",
                "  \"padding_scale\": 1.75,\n",
                "  \"background\": \"blurred\",\n",
                "  \"indicator\": false,\n",
                "  \"media_player\": false,\n",
                "  \"ai_usage\": false,\n",
                "  \"monitors\": {\n",
                "    \"cpu\": true,\n",
                "    \"memory\": true,\n",
                "    \"network\": true,\n",
                "    \"temp\": true,\n",
                "    \"disk\": false,\n",
                "    \"gpu\": false\n",
                "  }\n",
                "}\n"
            )
        );
    }

    #[test]
    fn the_marker_is_written_and_reread_identically() {
        let prefs = prefs_from("");
        let paths = scratch_paths("write");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);

        render_bar_layout(&cx).expect("render_bar_layout writes the marker");

        let written =
            std::fs::read_to_string(&paths.markers.bar_layout).expect("the marker was written");
        assert_eq!(
            written,
            format!("{}\n", json_dumps(&bar_layout_value(&prefs), 2))
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
