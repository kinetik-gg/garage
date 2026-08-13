//! `panel.toggle`: turn a Waybar click into the corresponding Quickshell IPC call.
//!
//! This is the Rust home of `garage-panel-toggle`'s former shell body. The action accepts
//! `{"panel": NAME, "widget": OPTIONAL_METRIC}`. The widget names which monitoring strip
//! was clicked, but does not steer the dashboard: all six metrics are shown and the surface
//! is anchored under the group rather than under one strip.
//!
//! Cursor and output geometry still come from Hyprland at click time. The monitor rectangle
//! is scale-aware and half-open, so a pointer on a shared edge belongs to exactly the output
//! on the far side. Waybar geometry comes from the generated CSS/JSON files. Every value read
//! from those files keeps the shipped fallback the shell had; a missing or hand-edited
//! runtime file may make an anchor less accurate, but must not turn a bar click into a no-op.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::command::{json_list, json_object, run};
use crate::cx::SessionCx;
use crate::error::ApplyError;

const METRICS: [&str; 6] = ["cpu", "memory", "network", "temp", "disk", "gpu"];
const PALETTE_WIDTH: i64 = 360;
const WORKSPACE_DOT: i64 = 6;
const ACTIVE_EXTRA: i64 = 14;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Panel {
    Notifications,
    ControlCenter,
    Monitor,
    Media,
    AiUsage,
}

impl Panel {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "notifications" => Some(Self::Notifications),
            "control-center" => Some(Self::ControlCenter),
            "monitor" => Some(Self::Monitor),
            "media" => Some(Self::Media),
            "ai-usage" => Some(Self::AiUsage),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Notifications => "notifications",
            Self::ControlCenter => "control-center",
            Self::Monitor => "monitor",
            Self::Media => "media",
            Self::AiUsage => "ai-usage",
        }
    }

    const fn function(self) -> &'static str {
        match self {
            Self::Notifications => "notificationsOn",
            Self::ControlCenter => "controlCenterOn",
            Self::Monitor => "monitorOn",
            Self::Media => "mediaOn",
            Self::AiUsage => "aiUsageOn",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct OutputTarget {
    name: String,
    logical_width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MonitorCss {
    image_padding: i64,
    tail_width: i64,
}

impl MonitorCss {
    const DEFAULT: Self = Self {
        image_padding: 8,
        tail_width: 320,
    };

    fn read(path: &Path) -> Self {
        let Some(css) = fs::read_to_string(path).ok() else {
            return Self::DEFAULT;
        };
        Self {
            image_padding: css_pixel(&css, "#image {", "padding: 0 ", "px;")
                .unwrap_or(Self::DEFAULT.image_padding),
            tail_width: css_pixel(&css, "#status-tail {", "min-width: ", "px;")
                .unwrap_or(Self::DEFAULT.tail_width),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MediaCss {
    menu_margin_left: i64,
    menu_min_width: i64,
    menu_padding_right: i64,
    menu_margin_right: i64,
    workspace_padding: i64,
    media_margin_left: i64,
}

impl MediaCss {
    const DEFAULT: Self = Self {
        menu_margin_left: 22,
        menu_min_width: 21,
        menu_padding_right: 6,
        menu_margin_right: 16,
        workspace_padding: 6,
        media_margin_left: 16,
    };

    fn read(path: &Path) -> Self {
        let Some(css) = fs::read_to_string(path).ok() else {
            return Self::DEFAULT;
        };
        Self {
            menu_margin_left: css_or(&css, "#custom-menu {", "margin-left: ", 22),
            menu_min_width: css_or(&css, "#custom-menu {", "min-width: ", 21),
            menu_padding_right: css_or(&css, "#custom-menu {", "padding-right: ", 6),
            menu_margin_right: css_or(&css, "#custom-menu {", "margin-right: ", 16),
            workspace_padding: css_or(&css, "#workspaces button {", "padding: 0 ", 6),
            media_margin_left: css_or(&css, "#custom-media {", "margin-left: ", 16),
        }
    }
}

/// Validate and run one panel toggle.
///
/// # Errors
///
/// [`ApplyError::Settings`] for a malformed request or when no monitor contains the cursor.
pub(crate) fn panel_toggle(cx: &SessionCx<'_>, value: Option<&Value>) -> Result<(), ApplyError> {
    let panel = request(value)?;
    let output = output_at_cursor(cx)?;
    let anchor = match panel {
        Panel::Monitor => Some(monitor_anchor(cx, &output)),
        Panel::Media => Some(media_anchor(cx, &output.name)),
        Panel::Notifications | Panel::ControlCenter | Panel::AiUsage => None,
    };
    call_shell(cx, panel.function(), &output.name, anchor);
    Ok(())
}

fn request(value: Option<&Value>) -> Result<Panel, ApplyError> {
    let payload = value
        .and_then(Value::as_object)
        .ok_or_else(|| setting("panel.toggle requires a JSON object"))?;
    let name = payload
        .get("panel")
        .and_then(Value::as_str)
        .ok_or_else(|| setting("panel.toggle requires a string panel"))?;
    let panel = Panel::parse(name).ok_or_else(|| setting(&format!("Unknown panel: {name}")))?;
    validate_widget(panel, payload.get("widget"))?;
    Ok(panel)
}

fn validate_widget(panel: Panel, widget: Option<&Value>) -> Result<(), ApplyError> {
    let Some(value) = widget.filter(|value| !value.is_null()) else {
        return Ok(());
    };
    if panel != Panel::Monitor {
        return Err(setting(&format!(
            "Panel {} does not accept a widget",
            panel.name()
        )));
    }
    let name = value
        .as_str()
        .ok_or_else(|| setting("panel.toggle requires widget to be a string"))?;
    if name.is_empty() || METRICS.contains(&name) {
        return Ok(());
    }
    Err(setting(&format!("Unknown monitor widget: {name}")))
}

fn setting(message: &str) -> ApplyError {
    ApplyError::Settings(message.to_owned())
}

fn output_at_cursor(cx: &SessionCx<'_>) -> Result<OutputTarget, ApplyError> {
    let cursor = json_object(cx, &["hyprctl", "cursorpos", "-j"]);
    let monitors = json_list(cx, &["hyprctl", "monitors", "-j"]);
    let point = point(&cursor).ok_or_else(|| setting("No monitor under cursor"))?;
    monitors
        .iter()
        .filter_map(monitor_geometry)
        .find(|(_, x, y, width, height)| {
            point.x >= *x && point.x < *x + *width && point.y >= *y && point.y < *y + *height
        })
        .map(|(name, _, _, logical_width, _)| OutputTarget {
            name,
            logical_width,
        })
        .ok_or_else(|| setting("No monitor under cursor"))
}

fn point(cursor: &Map<String, Value>) -> Option<Point> {
    Some(Point {
        x: cursor.get("x")?.as_f64()?,
        y: cursor.get("y")?.as_f64()?,
    })
}

fn monitor_geometry(value: &Value) -> Option<(String, f64, f64, f64, f64)> {
    let monitor = value.as_object()?;
    let scale = monitor.get("scale")?.as_f64()?;
    if scale <= 0.0 {
        return None;
    }
    Some((
        monitor.get("name")?.as_str()?.to_owned(),
        monitor.get("x")?.as_f64()?,
        monitor.get("y")?.as_f64()?,
        monitor.get("width")?.as_f64()? / scale,
        monitor.get("height")?.as_f64()? / scale,
    ))
}

/// The monitoring dashboard is anchored under the centre of Waybar's metric group. The
/// status tail follows that group and reaches a known distance in from the right edge, so
/// the centre is output width minus the tail minus half the sum of the strips. No cursor
/// coordinate participates: clicking either edge of a strip must place the surface alike.
fn monitor_anchor(cx: &SessionCx<'_>, output: &OutputTarget) -> i64 {
    let paths = cx.render().paths();
    let css = MonitorCss::read(&paths.config_home.join("waybar/style.css"));
    let module_padding = css.image_padding.saturating_mul(2);
    let metrics = read_metric_width(&paths.fragments.waybar_widgets, module_padding);
    floor_anchor(output.logical_width, css.tail_width, metrics)
}

fn read_metric_width(path: &Path, padding: i64) -> i64 {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| metric_width(&text, padding))
        .unwrap_or(0)
}

fn metric_width(text: &str, padding: i64) -> Option<i64> {
    let config = serde_json::from_str::<Value>(text).ok()?;
    let root = config.as_object()?;
    let modules = root.get("group/monitoring")?.get("modules")?.as_array()?;
    let mut width = 0_i64;
    for module in modules {
        let name = module.as_str()?;
        if !name.starts_with("image#metric-") {
            continue;
        }
        let size = root.get(name)?.get("size")?.as_i64()?;
        width = width.checked_add(size.checked_add(padding)?)?;
    }
    Some(width)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn floor_anchor(logical_width: f64, tail: i64, metrics: i64) -> i64 {
    (logical_width - tail as f64 - metrics as f64 / 2.0).floor() as i64
}

/// A media label changes with every title, so neither its centre nor the pointer is stable.
/// Align the palette's left edge with the module's stable leading edge instead: add the menu
/// geometry and the workspace buttons actually present on this output, then hand the palette
/// the centre of its fixed 360px width. Defaults are the generated 1.2/1.25 scale values.
fn media_anchor(cx: &SessionCx<'_>, output: &str) -> i64 {
    let paths = cx.render().paths();
    let css = MediaCss::read(&paths.config_home.join("waybar/style.css"));
    let workspace_width = if shows_workspaces(&paths.fragments.waybar_workspaces) {
        workspace_width(cx, output, css.workspace_padding)
    } else {
        0
    };
    [
        css.menu_margin_left,
        css.menu_min_width,
        css.menu_padding_right,
        css.menu_margin_right,
        workspace_width,
        css.media_margin_left,
        PALETTE_WIDTH / 2,
    ]
    .into_iter()
    .fold(0_i64, i64::saturating_add)
}

fn shows_workspaces(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| parsed_show_workspaces(&text))
        .unwrap_or(true)
}

fn parsed_show_workspaces(text: &str) -> Option<bool> {
    let config = serde_json::from_str::<Value>(text).ok()?;
    let modules = config.get("modules-left")?.as_array()?;
    Some(
        modules
            .iter()
            .any(|module| module.as_str() == Some("ext/workspaces")),
    )
}

fn workspace_width(cx: &SessionCx<'_>, output: &str, padding: i64) -> i64 {
    let count = json_list(cx, &["hyprctl", "workspaces", "-j"])
        .iter()
        .filter(|workspace| workspace.get("monitor").and_then(Value::as_str) == Some(output))
        .count();
    if count == 0 {
        return 0;
    }
    let count = i64::try_from(count).unwrap_or(i64::MAX);
    let button = WORKSPACE_DOT.saturating_add(padding.saturating_mul(2));
    count.saturating_mul(button).saturating_add(ACTIVE_EXTRA)
}

fn css_or(text: &str, selector: &str, property: &str, fallback: i64) -> i64 {
    css_pixel(text, selector, property, "px;").unwrap_or(fallback)
}

fn css_pixel(text: &str, selector: &str, property: &str, suffix: &str) -> Option<i64> {
    let mut inside = false;
    for line in text.lines() {
        if !inside {
            inside = line == selector;
            continue;
        }
        if line.starts_with('}') {
            return None;
        }
        let Some((_, tail)) = line.split_once(property) else {
            continue;
        };
        let Some((digits, _)) = tail.split_once(suffix) else {
            continue;
        };
        if !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit()) {
            return digits.parse().ok();
        }
    }
    None
}

fn call_shell(cx: &SessionCx<'_>, function: &str, output: &str, anchor: Option<i64>) {
    let anchor = anchor.map(|value| value.to_string());
    let mut command = vec![
        "qs", "-c", "garage", "ipc", "call", "shell", function, output,
    ];
    if let Some(value) = anchor.as_deref() {
        command.push(value);
    }
    // Let qs fail to stderr so a missing function stays visible. As with the shell's
    // `|| true`, the compositor binding itself still succeeds either way.
    drop(run(cx, &command));
}

#[cfg(test)]
mod tests;
