//! `render_bar_widgets()`: publish the bar's height, empty centre and its whole right side.
//!
//! A third fragment rather than more of either existing one, by the same rule
//! [`crate::bar::workspaces`] states: `waybar-clock.jsonc` is written on a region change and
//! `waybar-workspaces.jsonc` on a workspaces change, so a bar change that shared either file
//! would have to reproduce that writer's content or silently drop it. One writer per
//! fragment, three fragments.
//!
//! `height` lives here rather than in `config.jsonc`, for the reason `modules-left` is not
//! there either: an option a named file declares is won by that file, and an include can
//! never override it. Everything the bar section decides about the layout therefore has to
//! leave `config.jsonc` entirely.
//!
//! Written unconditionally, even with every widget switched off: the fragment is then the
//! bar's static tail and nothing else, which is a correct bar. An absent fragment is not --
//! `config.jsonc` no longer names `modules-right`, so it would be a bar with an empty right
//! side. `waybar.service` runs `garage render-bar` as its `ExecStartPre` for exactly that
//! reason.
//!
//! The module definitions this writes -- one `image#metric-*` module per enabled metric, the
//! AI usage strip, the media control -- now exec the Rust binaries in `$HOME/.local/bin`; the
//! old waybar Python paths died with the Python backend. This is also the one place a metric
//! strip's declared `size` has to agree with `garage-metrics`' own layout table;
//! `tests/test_bar.py` parses this script and fails on drift between the two.

use garage_core::fs::atomic::atomic_write;
use garage_core::schema::Preferences;
use garage_core::toml_emit::{json_dumps, Value};

use crate::cx::RenderCx;
use crate::error::RenderError;

/// `METRIC_STRIP_WIDTHS` (garage:2726-2732). waybar's image `size` is a fit-within box on the
/// LARGEST dimension: a 112x22 strip at size 22 renders 22px wide, so natural 1:1 rendering
/// needs `size` to equal the SVG's own width. The widths differ per widget because each strip
/// is drawn just wide enough for its worst-case value. These mirror `LAYOUTS[...]["width"]`
/// in `garage-metrics`; `tests/test_bar.py` parses that script and fails on drift.
const METRIC_STRIP_WIDTHS: &[(&str, i64)] = &[
    ("cpu", 82),
    ("memory", 82),
    ("temp", 76),
    ("disk", 91),
    ("gpu", 124),
    ("network", 130),
];

/// `BAR_METRICS` (garage:178). The metric strips the bar can carry, in the order they sit on
/// the right side. Each name is one of `garage-metrics`' own `--bar-svg` widgets, switched by
/// `bar.monitor_<name>`.
const BAR_METRICS: &[&str] = &["cpu", "memory", "network", "temp", "disk", "gpu"];

/// `WAYBAR_MONITOR_GROUP` (garage:151).
const WAYBAR_MONITOR_GROUP: &str = "group/monitoring";
/// `WAYBAR_STATUS_GROUP` (garage:152).
const WAYBAR_STATUS_GROUP: &str = "group/status-tail";
/// `WAYBAR_MODULES_RIGHT` (garage:149-150): the right side's fixed tail, in order, after
/// whatever widgets are switched on.
const WAYBAR_MODULES_RIGHT: &[&str] = &[
    "custom/notifications",
    "custom/launcher",
    "custom/control-center",
    "clock",
];

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

/// `METRIC_STRIP_WIDTHS[name]`. Unreachable for anything outside [`BAR_METRICS`], which is
/// the only source of a `name` this is ever called with.
fn strip_width(name: &str) -> i64 {
    METRIC_STRIP_WIDTHS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map_or(0, |(_, width)| *width)
}

/// One metric strip's module definition: the one `image#metric-*` form that works. `path` --
/// and every other spelling tried -- spins waybar at 100% CPU with no bar ever drawn; see the
/// note in `config.jsonc`. This is exec + interval + size, probed live.
fn metric_definition(name: &str) -> (String, Value) {
    (
        format!("image#metric-{name}"),
        Value::Table(vec![
            (
                "exec".to_owned(),
                Value::Str(format!("$HOME/.local/bin/garage-metrics --bar-svg {name}")),
            ),
            ("size".to_owned(), Value::Int(strip_width(name))),
            ("interval".to_owned(), Value::Int(2)),
            ("tooltip".to_owned(), Value::Bool(true)),
            (
                "on-click".to_owned(),
                Value::Str(format!(
                    "$HOME/.local/bin/garage-panel-toggle monitor {name}"
                )),
            ),
        ]),
    )
}

/// The AI usage strip. Minutes, not seconds: the figure behind it is a rolling usage total
/// that moves over a billing window, and tokscale is a subprocess per tick.
fn ai_usage_definition() -> (String, Value) {
    (
        "custom/ai-usage".to_owned(),
        Value::Table(vec![
            (
                "exec".to_owned(),
                Value::Str("$HOME/.local/bin/garage-ai-usage --bar".to_owned()),
            ),
            ("return-type".to_owned(), Value::Str("json".to_owned())),
            ("interval".to_owned(), Value::Int(300)),
            ("hide-empty-text".to_owned(), Value::Bool(true)),
            ("tooltip".to_owned(), Value::Bool(true)),
            (
                "on-click".to_owned(),
                Value::Str("$HOME/.local/bin/garage-panel-toggle ai-usage".to_owned()),
            ),
        ]),
    )
}

/// The media control. Listed after the workspace indicator by `render_bar_workspaces()`. Its
/// definition remains in this widget-owned fragment so the exec and all transport bindings
/// still switch off with `bar.media_player`.
fn media_definition() -> (String, Value) {
    (
        "custom/media".to_owned(),
        Value::Table(vec![
            (
                "exec".to_owned(),
                Value::Str("$HOME/.local/bin/garage-waybar-module media".to_owned()),
            ),
            ("return-type".to_owned(), Value::Str("json".to_owned())),
            ("interval".to_owned(), Value::Int(2)),
            ("hide-empty-text".to_owned(), Value::Bool(true)),
            ("tooltip".to_owned(), Value::Bool(true)),
            (
                "on-click".to_owned(),
                Value::Str("$HOME/.local/bin/garage-panel-toggle media".to_owned()),
            ),
            (
                "on-click-middle".to_owned(),
                Value::Str("/usr/bin/playerctl --player=playerctld play-pause".to_owned()),
            ),
            (
                "on-click-right".to_owned(),
                Value::Str("/usr/bin/playerctl --player=playerctld next".to_owned()),
            ),
            (
                "on-scroll-up".to_owned(),
                Value::Str("/usr/bin/playerctl --player=playerctld previous".to_owned()),
            ),
            (
                "on-scroll-down".to_owned(),
                Value::Str("/usr/bin/playerctl --player=playerctld next".to_owned()),
            ),
        ]),
    )
}

/// The two `group/*` wrappers: the metric strips on the left, the AI/status tail on the right.
fn group_definitions(metrics: &[String], status_tail: &[String]) -> [(String, Value); 2] {
    let monitor = (
        WAYBAR_MONITOR_GROUP.to_owned(),
        Value::Table(vec![
            (
                "orientation".to_owned(),
                Value::Str("horizontal".to_owned()),
            ),
            (
                "modules".to_owned(),
                Value::Array(metrics.iter().cloned().map(Value::Str).collect()),
            ),
        ]),
    );
    let status_modules: Vec<Value> = status_tail
        .iter()
        .cloned()
        .chain(WAYBAR_MODULES_RIGHT.iter().map(|name| (*name).to_owned()))
        .map(Value::Str)
        .collect();
    let status = (
        WAYBAR_STATUS_GROUP.to_owned(),
        Value::Table(vec![
            (
                "orientation".to_owned(),
                Value::Str("horizontal".to_owned()),
            ),
            ("modules".to_owned(), Value::Array(status_modules)),
        ]),
    );
    [monitor, status]
}

/// The bar's right side, empty centre and widget definitions (`bar_widget_modules()`,
/// garage:2736-2808).
///
/// Split out from the renderer so the order can be asserted without writing a file. The
/// order is [`BAR_METRICS`]' order, then the AI strip, then [`WAYBAR_MODULES_RIGHT`] -- the
/// static tail is last so the notification bell, the launcher and the clock stay where they
/// have always been however many widgets are switched on in front of them.
///
/// Only the modules that are actually listed are defined. waybar does not mind a definition
/// for a module nobody lists, but a fragment that carries six exec lines while the bar runs
/// three of them reads as though all six are running.
pub(crate) fn bar_widget_modules(prefs: &Preferences) -> (Vec<String>, Vec<String>, Value) {
    let bar = &prefs.bar;
    let mut metrics: Vec<String> = Vec::new();
    let mut status_tail: Vec<String> = Vec::new();
    let mut definitions: Vec<(String, Value)> = Vec::new();

    for &name in BAR_METRICS {
        if !monitor_enabled(bar, name) {
            continue;
        }
        let (module, definition) = metric_definition(name);
        metrics.push(module.clone());
        definitions.push((module, definition));
    }

    if bar.ai_usage {
        status_tail.push("custom/ai-usage".to_owned());
        definitions.push(ai_usage_definition());
    }

    let right: Vec<String> = vec![
        WAYBAR_MONITOR_GROUP.to_owned(),
        WAYBAR_STATUS_GROUP.to_owned(),
    ];
    definitions.extend(group_definitions(&metrics, &status_tail));

    let center: Vec<String> = Vec::new();
    if bar.media_player {
        definitions.push(media_definition());
    }

    (right, center, Value::Table(definitions))
}

/// Write the bar's height, empty centre and its whole right side (`render_bar_widgets()`,
/// garage:2809-2840).
///
/// # Errors
///
/// [`RenderError::Atomic`] if the fragment cannot be replaced.
pub fn render_bar_widgets(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let (right, center, definitions) = bar_widget_modules(cx.prefs());
    let mut fragment: Vec<(String, Value)> = vec![
        ("height".to_owned(), Value::Int(cx.prefs().bar.height.get())),
        (
            "modules-center".to_owned(),
            Value::Array(center.into_iter().map(Value::Str).collect()),
        ),
        (
            "modules-right".to_owned(),
            Value::Array(right.into_iter().map(Value::Str).collect()),
        ),
    ];
    let Value::Table(definitions) = definitions else {
        unreachable!("bar_widget_modules always returns a Value::Table")
    };
    fragment.extend(definitions);
    atomic_write(
        &cx.paths().fragments.waybar_widgets,
        &format!("{}\n", json_dumps(&Value::Table(fragment), 2)),
    )?;
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
    use garage_core::toml_emit::Value;
    use garage_core::traits::{
        LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource,
    };

    use super::{bar_widget_modules, render_bar_widgets};
    use crate::cx::RenderCx;

    /// `desktop/.local/bin/garage`'s own `render_bar_widgets(FALLBACK_DEFAULTS)` output,
    /// captured with `tests/harness.py`'s `load_backend()` -- see this task's report for the
    /// throwaway script. Pinned as a file rather than inlined so the exact `CPython`
    /// `json.dumps(indent=2)` bytes -- key order, the trailing newline, the two-space
    /// indent -- are checked, not just the logical structure `bar_widget_modules()`'s own
    /// tests already cover.
    const DEFAULTS_FIXTURE: &str = include_str!("../../testdata/bar_widgets_defaults.jsonc");

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

    #[test]
    fn render_bar_widgets_matches_the_python_backend_byte_for_byte_on_the_shipped_defaults() {
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let home = std::env::temp_dir().join(format!(
            "garage-render-bar-widgets-parity-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        let paths = Paths::from_env_map(&env);
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(defaults.values(), &paths, &monitors, &lua);
        render_bar_widgets(&cx).expect("render_bar_widgets succeeds on a clean scratch");
        let found = std::fs::read_to_string(&paths.fragments.waybar_widgets)
            .expect("the fragment was written");
        assert_eq!(found, DEFAULTS_FIXTURE);
        drop(std::fs::remove_dir_all(&paths.home));
    }

    fn prefs_from(departures: &str) -> Preferences {
        let table: toml::Table = departures.parse().expect("fixture toml parses");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let mut notes = Notes::new();
        Preferences::coerce_from(&table, &defaults, &mut notes)
    }

    fn definition_names(value: &Value) -> Vec<String> {
        let Value::Table(entries) = value else {
            panic!("bar_widget_modules always returns a Value::Table");
        };
        entries.iter().map(|(name, _)| name.clone()).collect()
    }

    #[test]
    fn the_shipped_defaults_enable_cpu_memory_network_ai_usage_and_media_and_the_tail_is_last() {
        let prefs = prefs_from("");
        let (right, center, definitions) = bar_widget_modules(&prefs);
        assert_eq!(right, ["group/monitoring", "group/status-tail"]);
        assert!(center.is_empty());
        assert_eq!(
            definition_names(&definitions),
            [
                "image#metric-cpu",
                "image#metric-memory",
                "image#metric-network",
                "custom/ai-usage",
                "group/monitoring",
                "group/status-tail",
                "custom/media",
            ]
        );
    }

    #[test]
    fn every_widget_off_leaves_only_the_two_groups() {
        let prefs = prefs_from(
            "[bar]\nmonitor_cpu = false\nmonitor_memory = false\nmonitor_network = false\n\
             ai_usage = false\nmedia_player = false\n",
        );
        let (_, _, definitions) = bar_widget_modules(&prefs);
        assert_eq!(
            definition_names(&definitions),
            ["group/monitoring", "group/status-tail"]
        );
    }

    #[test]
    fn media_player_off_drops_only_its_own_definition() {
        let prefs = prefs_from("[bar]\nmedia_player = false\n");
        let (_, _, definitions) = bar_widget_modules(&prefs);
        let names = definition_names(&definitions);
        assert!(!names.iter().any(|name| name == "custom/media"));
    }

    #[test]
    fn every_metric_on_orders_by_bar_metrics_not_schema_declaration_order() {
        let prefs = prefs_from(
            "[bar]\nmonitor_temp = true\nmonitor_disk = true\nmonitor_gpu = true\n\
             ai_usage = false\nmedia_player = false\n",
        );
        let (_, _, definitions) = bar_widget_modules(&prefs);
        assert_eq!(
            definition_names(&definitions),
            [
                "image#metric-cpu",
                "image#metric-memory",
                "image#metric-network",
                "image#metric-temp",
                "image#metric-disk",
                "image#metric-gpu",
                "group/monitoring",
                "group/status-tail",
            ]
        );
    }
}
