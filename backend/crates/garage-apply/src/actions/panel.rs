//! `panel.toggle`: turn a keybind into the corresponding Quickshell IPC call.
//!
//! This is the Rust home of `garage-panel-toggle`'s shell body. The action accepts
//! `{"panel": NAME, "widget": OPTIONAL_METRIC}`; the widget names which monitoring strip
//! was asked for but does not steer the dashboard, exactly as before.
//!
//! Since the bar moved into the shell, a bar click no longer passes through here at all:
//! it calls the same shell functions in-process with its own anchor coordinates. What
//! remains is the keybind path, and its contract is unchanged: the panel opens on the
//! monitor under the pointer -- cursor position and output geometry still come from
//! Hyprland at keypress time, scale-aware and half-open, so a pointer on a shared edge
//! belongs to exactly one output. Anchored panels are handed `-1`, which is the shell's
//! "no click coordinate; centre yourself" value.

use serde_json::Value;

use super::hyprland::output_at_cursor;
use crate::command::run;
use crate::cx::SessionCx;
use crate::error::ApplyError;

const METRICS: [&str; 6] = ["cpu", "memory", "network", "temp", "disk", "gpu"];

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

    /// The two panels whose palettes anchor under a bar module take the shell's
    /// explicit no-anchor value; the rest take only a screen name.
    const fn anchored(self) -> bool {
        matches!(self, Self::Monitor | Self::Media)
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
    call_shell(cx, panel, output.name.as_str());
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

fn call_shell(cx: &SessionCx<'_>, panel: Panel, output: &str) {
    let mut command = vec![
        "qs",
        "-c",
        "garage",
        "ipc",
        "call",
        "shell",
        panel.function(),
        output,
    ];
    if panel.anchored() {
        command.push("-1");
    }
    // Let qs fail to stderr so a missing function stays visible. As with the shell's
    // `|| true`, the compositor binding itself still succeeds either way.
    drop(run(cx, &command));
}
