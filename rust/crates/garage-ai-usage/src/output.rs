//! `build_bar_output()`, `build_json_output()`, `run_probe()`: the three things `main()`
//! can be asked to print.

use std::path::Path;

use serde_json::{Map, Value};

use crate::cache::{self, CachePaths};
use crate::shape;

/// The bar module is this one glyph -- Phosphor's "sparkle" -- and nothing else.
///
/// The percentages and reset days it used to spell out ("OAI 99% 6D ANT 89% 7D") were four
/// numbers nobody reads at a glance, and they are still a hover away in the tooltip built
/// by [`build_bar_output`] and a click away in the AI palette. The codepoint is Phosphor's
/// own private-use assignment, verified present in the bundled
/// `desktop/.local/share/fonts/Phosphor.ttf`, and `waybar-base.css` puts "Phosphor" first
/// in this module's font stack so nothing else can claim it. Written as an escape, the way
/// `config.jsonc` spells the bell and the launcher: a private-use glyph pasted into source
/// is invisible in a diff and one careless editor away from being lost.
const SPARKLE: char = '\u{e6a2}';

fn object<const N: usize>(entries: [(&str, Value); N]) -> Value {
    let mut map = Map::new();
    for (key, entry) in entries {
        map.insert(key.to_string(), entry);
    }
    Value::Object(map)
}

fn is_empty_list(value: &Value) -> bool {
    value.as_array().is_none_or(Vec::is_empty)
}

/// `run_probe()`: locate the tokscale CLI and report it, matching stdout/stderr and the
/// exit code the Python's `run_probe()` produces.
pub(crate) fn run_probe(tokscale: Option<&Path>) -> i32 {
    match tokscale {
        None => {
            eprintln!(
                "tokscale not found: checked ~/.local/share/tokscale/node_modules/.bin/tokscale and PATH"
            );
            1
        }
        Some(path) => {
            println!("{}", path.display());
            0
        }
    }
}

/// `build_bar_output()`: the waybar custom-module payload.
///
/// `now_epoch_seconds` is the wall clock [`shape::reset_days`] measures against; `main()`
/// supplies the real one, tests a fixed one.
pub(crate) fn build_bar_output(
    tokscale: Option<&Path>,
    paths: &CachePaths,
    now_epoch_seconds: f64,
) -> Value {
    let Some(tokscale) = tokscale else {
        return object([
            ("text", Value::String(String::new())),
            ("tooltip", Value::String(String::new())),
            ("class", Value::String("unavailable".to_string())),
        ]);
    };

    let (payload, stale) = cache::load_usage(tokscale, paths);

    if is_empty_list(&payload) {
        // Still the glyph, not "AI --": the module is an icon now, and a strip that
        // changes shape when a subprocess fails is a worse signal than the tooltip saying
        // so. The class is there for a palette that wants to dim it.
        return object([
            ("text", Value::String(SPARKLE.to_string())),
            (
                "tooltip",
                Value::String("Tokscale subscription usage unavailable".to_string()),
            ),
            ("class", Value::String("unavailable".to_string())),
        ]);
    }

    let codex = shape::provider(&payload, "Codex");
    let claude = shape::provider(&payload, "Claude");
    let codex_plan = codex
        .get("plan")
        .and_then(Value::as_str)
        .unwrap_or("unknown plan");
    let claude_plan = claude
        .get("plan")
        .and_then(Value::as_str)
        .unwrap_or("unknown plan");
    let freshness = if stale { " (cached)" } else { "" };

    let tooltip = format!(
        "OAI / Codex ({codex_plan})\n  Weekly: {} remaining ({})\n  Reset: {}\n  Session: {} remaining\nANT / Claude ({claude_plan})\n  Weekly: {} remaining ({})\n  Reset: {}\n  Session: {} remaining\nSource: Tokscale{freshness}",
        shape::percent(&codex, "Weekly"),
        shape::reset_days(&codex, "Weekly", now_epoch_seconds),
        shape::reset(&codex, "Weekly"),
        shape::percent(&codex, "Session"),
        shape::percent(&claude, "Weekly"),
        shape::reset_days(&claude, "Weekly", now_epoch_seconds),
        shape::reset(&claude, "Weekly"),
        shape::percent(&claude, "Session"),
    );

    object([
        ("text", Value::String(SPARKLE.to_string())),
        ("tooltip", Value::String(tooltip)),
        (
            "class",
            Value::String(if stale { "stale" } else { "available" }.to_string()),
        ),
    ])
}

/// `build_json_output()`: the popover payload.
///
/// `fetched_at` is `datetime.now(timezone.utc).isoformat()`'s Rust equivalent, formatted by
/// [`crate::timeutil::format_fetched_at_utc`]; `main()` supplies the real wall clock.
pub(crate) fn build_json_output(
    tokscale: Option<&Path>,
    paths: &CachePaths,
    fetched_at: String,
) -> Value {
    let Some(tokscale) = tokscale else {
        return object([("available", Value::Bool(false))]);
    };

    let (payload, stale) = cache::load_usage(tokscale, paths);
    let today = cache::load_today(tokscale, paths);
    let available = !is_empty_list(&payload);

    object([
        ("available", Value::Bool(available)),
        ("fetched_at", Value::String(fetched_at)),
        ("stale", Value::Bool(stale)),
        ("subscriptions", payload),
        ("today", today.unwrap_or(Value::Null)),
    ])
}

#[cfg(test)]
mod tests {
    use super::{build_bar_output, build_json_output, run_probe, SPARKLE};
    use crate::cache::CachePaths;
    use std::fs;
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "garage-ai-usage-output-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&path).expect("scratch directory is creatable");
        path
    }

    fn write_shell_script(path: &std::path::Path, body: &str) {
        let mut file = fs::File::create(path).expect("script is creatable");
        file.write_all(format!("#!/bin/sh\n{body}\n").as_bytes())
            .expect("script is writable");
        let mut permissions = fs::metadata(path).expect("script metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("script is chmod-able");
    }

    /// `value[key]`, without indexing syntax: `Value::get` returns `Option<&Value>`, which
    /// this crate's lint wall (`clippy::indexing_slicing`) requires over the panicking
    /// `Index` impl -- even in tests, where `.expect()` alone is allowed.
    fn field(value: &serde_json::Value, key: &str) -> serde_json::Value {
        value.get(key).cloned().unwrap_or(serde_json::Value::Null)
    }

    #[test]
    fn bar_output_is_empty_when_tokscale_is_absent() {
        let dir = scratch("bar-absent");
        let paths = CachePaths::new(dir.join("cache"));
        let value = build_bar_output(None, &paths, 0.0);
        assert_eq!(
            value,
            serde_json::json!({"text": "", "tooltip": "", "class": "unavailable"})
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bar_output_is_the_glyph_with_an_unavailable_class_when_nothing_validates() {
        let dir = scratch("bar-unavailable");
        let script = dir.join("tokscale");
        write_shell_script(&script, "echo '[]'");
        let paths = CachePaths::new(dir.join("cache"));

        let value = build_bar_output(Some(&script), &paths, 0.0);
        assert_eq!(
            field(&value, "text"),
            serde_json::json!(SPARKLE.to_string())
        );
        assert_eq!(field(&value, "class"), serde_json::json!("unavailable"));
        assert_eq!(
            field(&value, "tooltip"),
            serde_json::json!("Tokscale subscription usage unavailable")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bar_output_reports_percentages_and_reset_days_when_available() {
        let dir = scratch("bar-available");
        let script = dir.join("tokscale");
        write_shell_script(
            &script,
            r#"echo '[{"provider": "Codex", "plan": "pro", "metrics": [{"label": "Weekly", "remaining_percent": 91.0, "resets_at": "2024-06-08T00:00:00Z"}, {"label": "Session", "remaining_percent": 50.0}]}]'"#,
        );
        let paths = CachePaths::new(dir.join("cache"));
        let now = crate::timeutil::parse_iso8601_utc_seconds("2024-06-02T00:00:00Z")
            .expect("fixture parses");

        let value = build_bar_output(Some(&script), &paths, now);
        assert_eq!(
            field(&value, "text"),
            serde_json::json!(SPARKLE.to_string())
        );
        assert_eq!(field(&value, "class"), serde_json::json!("available"));
        let tooltip_value = field(&value, "tooltip");
        let tooltip = tooltip_value.as_str().expect("tooltip is a string");
        assert!(tooltip.contains("OAI / Codex (pro)"));
        assert!(tooltip.contains("91% remaining (6D)"));
        assert!(tooltip.contains("ANT / Claude (unknown plan)"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_output_reports_unavailable_when_tokscale_is_absent() {
        let dir = scratch("json-absent");
        let paths = CachePaths::new(dir.join("cache"));
        let value = build_json_output(None, &paths, "irrelevant".to_string());
        assert_eq!(value, serde_json::json!({"available": false}));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_output_passes_subscriptions_and_today_through() {
        let dir = scratch("json-available");
        let script = dir.join("tokscale");
        write_shell_script(
            &script,
            r#"
if [ "$2" = "--json" ] && [ "$1" = "usage" ]; then
  echo '[{"provider": "Codex", "plan": "pro"}]'
else
  echo '{"total_cost": 4.5}'
fi
"#,
        );
        let paths = CachePaths::new(dir.join("cache"));

        let value = build_json_output(
            Some(&script),
            &paths,
            "2024-06-01T00:00:00+00:00".to_string(),
        );
        assert_eq!(field(&value, "available"), serde_json::json!(true));
        assert_eq!(field(&value, "stale"), serde_json::json!(false));
        assert_eq!(
            field(&value, "subscriptions"),
            serde_json::json!([{"provider": "Codex", "plan": "pro"}])
        );
        assert_eq!(
            field(&value, "today"),
            serde_json::json!({"total_cost": 4.5})
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_reports_the_path_and_exits_zero() {
        let path = PathBuf::from("/usr/bin/tokscale");
        assert_eq!(run_probe(Some(&path)), 0);
    }

    #[test]
    fn probe_exits_nonzero_when_absent() {
        assert_eq!(run_probe(None), 1);
    }
}
