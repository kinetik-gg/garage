//! `audio_snapshot()`: every sink and source `pactl` reports, simplified to what the pane
//! shows.
//!
//! Reads `pactl -f json info`, `list sinks` and `list sources`, then reduces each device to a
//! name, a description, whether it is the default, mute state and a volume fraction. The
//! volume comes from the first channel's `value_percent` -- devices report per-channel
//! volumes and the pane draws one slider, so the first channel stands for all of them, which
//! is the same simplification a stereo balance control would otherwise need a second slider
//! to avoid.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.

use serde_json::{json, Map, Value};

use crate::command::{json_list, json_object};
use crate::cx::SessionCx;

/// `audio_snapshot()` (garage:5040-5063): every sink and source, simplified to what the pane
/// shows.
pub(crate) fn audio_snapshot(cx: &SessionCx<'_>) -> Value {
    let info = json_object(cx, &["pactl", "-f", "json", "info"]);
    let sinks = json_list(cx, &["pactl", "-f", "json", "list", "sinks"]);
    let sources = json_list(cx, &["pactl", "-f", "json", "list", "sources"]);
    json!({
        "outputs": simplify(&sinks, &text(&info, "default_sink_name")),
        "inputs": simplify(&sources, &text(&info, "default_source_name")),
    })
}

/// `str(info.get(key, ""))` for the two default-device names.
fn text(info: &Map<String, Value>, key: &str) -> String {
    info.get(key)
        .map(|value| {
            value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned)
        })
        .unwrap_or_default()
}

/// `simplify()`: one device record, reduced to the five fields the pane draws.
fn simplify(items: &[Value], default_name: &str) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| {
                let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                json!({
                    "name": name,
                    // `item.get("description", item.get("name", "Device"))`: the fallback
                    // chain is a *default argument*, so a record with no description at all
                    // shows its device name, and one with neither shows "Device".
                    "description": item
                        .get("description")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("name").and_then(Value::as_str))
                        .unwrap_or("Device"),
                    "default": item.get("name").and_then(Value::as_str) == Some(default_name),
                    "mute": item.get("mute").is_some_and(|found| found == &Value::Bool(true)),
                    "volume": first_channel_volume(item),
                })
            })
            .collect(),
    )
}

/// The first channel's `value_percent`, as a fraction.
///
/// Devices report a volume per channel and the pane draws one slider, so the first channel
/// stands for all of them -- the same simplification a stereo balance control would otherwise
/// need a second slider to avoid. `"65%"` is stripped to `"65"` and divided; an empty string
/// reads as zero, which is `float(percent or 0)`.
fn first_channel_volume(item: &Value) -> f64 {
    let percent = item
        .get("volume")
        .and_then(Value::as_object)
        .and_then(|channels| channels.values().next())
        .and_then(|channel| channel.get("value_percent"))
        .map_or_else(
            || "0".to_owned(),
            |value| {
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned)
                    .trim_end_matches('%')
                    .to_owned()
            },
        );
    if percent.is_empty() {
        return 0.0;
    }
    percent.parse::<f64>().unwrap_or_default() / 100.0
}
