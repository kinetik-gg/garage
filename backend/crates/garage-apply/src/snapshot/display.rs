//! `display_snapshot()`: every monitor Hyprland reports, folded with the saved layout.
//!
//! Queries `hyprctl monitors all` rather than the default list: Hyprland pulls a mirrored
//! output out of its monitor layout entirely -- it loses its `wl_output` too, so `grim`
//! cannot see it either -- and a disabled one never appears there either. Both would vanish
//! from the pane with no control left to turn them back on if the default list were used
//! instead.
//!
//! `mirrorOf` is reported as the source's *id*, rendered as a string, with the literal
//! `"none"` for a display that mirrors nothing; resolving it through an id-to-name map is
//! what turns that back into an output name. `mirrorOf` is never tested for truthiness, only
//! looked up -- id `0` is a real monitor, and truthiness would misread it as absent, while
//! `"none"` simply misses the map, which is also the answer for a display that mirrors
//! nothing.
//!
//! A mirror is reported at its source's position, so the spot it holds in the arrangement
//! only survives in `displays.toml` -- the snapshot substitutes the saved position back in
//! for a mirrored output, or turning the mirror off would drop the display on top of the very
//! output it was copying.
//!
//! `vrr` comes from `displays.toml` and not from the monitor: `hyprctl`'s `vrr` field is the
//! output's live adaptive-sync *state*, not the mode that was asked for -- it stays false for
//! a fullscreen-only mode with nothing fullscreen, and on a panel whose CRTC cannot do VRR at
//! all. The mode itself only survives in the file, where `-1` means follow `misc:vrr`.
//!
//! If nothing in the result is marked primary, the focused display is promoted to primary
//! (falling back to the first) so the pane always has exactly one primary to show, even on a
//! machine that has never saved a layout.
//!
//! Doc-only in the dispatch sense: this returns a snapshot value for [`crate::snapshot`]'s
//! JSON envelope rather than being a [`Route`](garage_core::schema::routes::Route) step, and
//! is reached from `make_snapshot()` -- and, on the apply side, from
//! `initialize_display_config()`, which seeds `displays.toml` from it on a machine that has
//! never had one.

use garage_core::pyrepr::py_format_g;
use garage_render::displays::{load_display_config, DisplayEntry, DisplayLayout, LayoutValue};
use serde_json::Value;

use crate::command::json_list;
use crate::cx::SessionCx;
use crate::displays::wire::from_json;

/// `display_snapshot()` (garage:4956-5015): the arrangement the Displays pane draws.
///
/// `primary_environment` is `HYPR_PRIMARY_MONITOR`, read by the caller rather than here so a
/// test can name it without reaching for the process environment -- the same shape
/// `workspace_outputs()` uses one crate down.
///
/// Nothing here fails. Every read the Python makes is already lenient: `json_command()` folds
/// a missing `hyprctl`, a timeout, a non-zero exit and unparseable output into one empty
/// list, and `load_display_config()` is wrapped in `try/except SettingsError` so an
/// unreadable `displays.toml` contributes an empty layout rather than taking the pane down.
pub(crate) fn display_snapshot(cx: &SessionCx<'_>, primary_environment: &str) -> Vec<DisplayEntry> {
    let monitors = json_list(cx, &["hyprctl", "monitors", "all", "-j"]);
    // `{str(monitor.get("id")): str(monitor.get("name", "")) for monitor in monitors if
    // isinstance(monitor, dict)}` -- `str(None)` is `"None"` for a record with no id, which
    // no `mirrorOf` can name.
    let by_id: Vec<(String, String)> = monitors
        .iter()
        .filter(|monitor| monitor.is_object())
        .map(|monitor| (py_str_of(monitor.get("id")), text_of(monitor, "name")))
        .collect();
    let saved = load_display_config(&cx.render().paths().host.displays).unwrap_or_default();
    let primary = if saved.primary.is_empty() {
        primary_environment.to_owned()
    } else {
        saved.primary.clone()
    };

    let mut result: Vec<DisplayEntry> = Vec::new();
    for monitor in &monitors {
        let mirror = by_id
            .iter()
            .find(|(id, _)| *id == py_str_of(monitor.get("mirrorOf")))
            .map_or_else(String::new, |(_, source)| source.clone());
        result.push(record_for(
            monitor,
            saved_record(&saved, monitor.get("name")).as_ref(),
            &mirror,
            &primary,
        ));
    }
    promote_a_primary(&mut result);
    result
}

/// One monitor's record, folded with the saved one for the same output.
fn record_for(
    monitor: &Value,
    configured: Option<&DisplayEntry>,
    mirror: &str,
    primary: &str,
) -> DisplayEntry {
    let mut fields = identity_fields(monitor);
    fields.extend(geometry_fields(monitor, configured, mirror));
    fields.push((
        "modes".to_owned(),
        field(monitor, "availableModes", LayoutValue::Array(Vec::new())),
    ));
    fields.push((
        "focused".to_owned(),
        field(monitor, "focused", LayoutValue::Bool(false)),
    ));
    fields.push((
        "primary".to_owned(),
        LayoutValue::Bool(text_of(monitor, "name") == primary),
    ));
    DisplayEntry::from_fields(fields)
}

/// Who the display is: the five identity fields plus whether it is switched on.
fn identity_fields(monitor: &Value) -> Vec<(String, LayoutValue)> {
    let empty = || LayoutValue::Str(String::new());
    vec![
        ("output".to_owned(), field(monitor, "name", empty())),
        (
            "description".to_owned(),
            // `monitor.get("description", monitor.get("name", "Display"))`: the name stands in
            // for a missing description, and "Display" only for a record that has neither.
            match monitor.get("description") {
                Some(found) => from_json(found),
                None => field(monitor, "name", LayoutValue::Str("Display".to_owned())),
            },
        ),
        ("make".to_owned(), field(monitor, "make", empty())),
        ("model".to_owned(), field(monitor, "model", empty())),
        ("serial".to_owned(), field(monitor, "serial", empty())),
        (
            "enabled".to_owned(),
            // `not monitor.get("disabled", False)`, at Python's truthiness.
            LayoutValue::Bool(!truthy_field(monitor, "disabled")),
        ),
    ]
}

/// Where the display is and what it is showing.
fn geometry_fields(
    monitor: &Value,
    configured: Option<&DisplayEntry>,
    mirror: &str,
) -> Vec<(String, LayoutValue)> {
    // A mirror is reported at its source's position, so the saved record is where the spot it
    // holds in the arrangement survives.
    let placed: Option<&DisplayEntry> = if mirror.is_empty() { None } else { configured };
    let current_mode = format!(
        "{}x{}@{}",
        integer_of(monitor, "width", 0),
        integer_of(monitor, "height", 0),
        py_format_g(float_of(monitor, "refreshRate", 60.0))
    );
    let saved = |key: &str| configured.and_then(|record| record.get(key).cloned());
    vec![
        (
            "width".to_owned(),
            field(monitor, "width", LayoutValue::Int(0)),
        ),
        (
            "height".to_owned(),
            field(monitor, "height", LayoutValue::Int(0)),
        ),
        (
            "refresh".to_owned(),
            field(monitor, "refreshRate", LayoutValue::Int(60)),
        ),
        (
            "mode".to_owned(),
            saved("mode").unwrap_or(LayoutValue::Str(current_mode)),
        ),
        ("mirror".to_owned(), LayoutValue::Str(mirror.to_owned())),
        ("x".to_owned(), placed_field(placed, monitor, "x")),
        ("y".to_owned(), placed_field(placed, monitor, "y")),
        (
            "scale".to_owned(),
            field(monitor, "scale", LayoutValue::Int(1)),
        ),
        (
            "transform".to_owned(),
            field(monitor, "transform", LayoutValue::Int(0)),
        ),
        (
            "vrr".to_owned(),
            // `int(configured.get("vrr", -1))`, and the one field read from the file rather
            // than from the compositor -- see the module doc.
            LayoutValue::Int(
                saved("vrr")
                    .map_or(Ok(-1), |held| held.py_int())
                    .unwrap_or(-1),
            ),
        ),
    ]
}

/// `bool(monitor.get(key))`, for the one field read through Python's truthiness.
fn truthy_field(monitor: &Value, key: &str) -> bool {
    monitor
        .get(key)
        .is_some_and(|held| from_json(held).truthy())
}

/// `if result and not any(item["primary"] ...)`: the focused display, or the first.
fn promote_a_primary(result: &mut [DisplayEntry]) {
    if result.is_empty()
        || result
            .iter()
            .any(|entry| entry.get("primary").is_some_and(LayoutValue::truthy))
    {
        return;
    }
    let at = result
        .iter()
        .position(|entry| entry.get("focused").is_some_and(LayoutValue::truthy))
        .unwrap_or(0);
    if let Some(entry) = result.get_mut(at) {
        entry.set("primary", LayoutValue::Bool(true));
    }
}

/// The saved record for this monitor: `saved_by_output.get(monitor.get("name"), {})`, which
/// matches on the raw JSON value rather than on a coerced string -- a monitor with no name at
/// all matches a saved record with no output at all, exactly as `None == None` does.
fn saved_record(saved: &DisplayLayout, name: Option<&Value>) -> Option<DisplayEntry> {
    let wanted = name.map(from_json);
    saved
        .displays
        .iter()
        .find(|entry| entry.get("output").cloned() == wanted)
        .cloned()
}

/// `monitor.get(key, fallback)` as a layout value.
fn field(monitor: &Value, key: &str, fallback: LayoutValue) -> LayoutValue {
    monitor.get(key).map_or(fallback, from_json)
}

/// `placed.get(key, 0)`, where `placed` is the saved record for a mirror and the live monitor
/// for everything else.
fn placed_field(placed: Option<&DisplayEntry>, monitor: &Value, key: &str) -> LayoutValue {
    match placed {
        Some(record) => record.get(key).cloned().unwrap_or(LayoutValue::Int(0)),
        None => field(monitor, key, LayoutValue::Int(0)),
    }
}

/// `str(monitor.get(key))`, including `"None"` for an absent key -- which is what the id map
/// and the `mirrorOf` lookup are both built on.
fn py_str_of(value: Option<&Value>) -> String {
    value.map_or_else(|| "None".to_owned(), |held| from_json(held).py_str())
}

/// `str(monitor.get(key, ""))`.
fn text_of(monitor: &Value, key: &str) -> String {
    monitor
        .get(key)
        .map_or_else(String::new, |held| from_json(held).py_str())
}

/// `int(monitor.get(key, fallback))`, with a value the conversion refuses falling back rather
/// than raising -- the Python would raise, and nothing in `hyprctl`'s own output can.
fn integer_of(monitor: &Value, key: &str, fallback: i64) -> i64 {
    monitor
        .get(key)
        .map_or(Ok(fallback), |held| from_json(held).py_int())
        .unwrap_or(fallback)
}

/// `float(monitor.get(key, fallback))`, with the same fallback reading as [`integer_of`].
fn float_of(monitor: &Value, key: &str, fallback: f64) -> f64 {
    monitor
        .get(key)
        .map_or(Ok(fallback), |held| from_json(held).py_float())
        .unwrap_or(fallback)
}
