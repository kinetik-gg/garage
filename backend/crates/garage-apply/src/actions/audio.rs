//! The six audio actions: two volumes, two mutes, two default devices.
//!
//! Two tools, and the split is not arbitrary. Volume and mute go to `wpctl`, `PipeWire`'s own
//! control, addressed through the `@DEFAULT_AUDIO_SINK@` / `@DEFAULT_AUDIO_SOURCE@` aliases
//! -- so the pane never has to name a device to change the volume of the one in use, and a
//! device swapped between the read and the write moves the right slider. Choosing the default
//! device itself goes to `pactl` instead, which is where that concept lives: `PipeWire`'s
//! pulse server owns the default-sink and default-source properties, and `wpctl` has no verb
//! for them.
//!
//! All six are `check=True`, unlike almost everything else this crate runs: an audio change
//! the user just made either happened or did not, and reporting nothing would leave the pane
//! showing a slider position the machine does not have.

use serde_json::Value;

use crate::actions::pyvalue::{py_float_argument, py_str, py_truthy};
use crate::command::run_checked;
use crate::cx::SessionCx;
use crate::error::ApplyError;

/// The six names this module answers, so [`crate::actions::action`]'s dispatch can ask
/// whether a name is one of them without repeating the list.
pub(crate) const NAMES: [&str; 6] = [
    "audio.output.volume",
    "audio.input.volume",
    "audio.output.mute",
    "audio.input.mute",
    "audio.output.default",
    "audio.input.default",
];

/// Run one audio action (garage:5347-5358).
///
/// # Errors
///
/// [`ApplyError::Settings`] carrying `float()`'s own refusal for an unusable volume payload,
/// or the `CalledProcessError` text `run(check=True)` produces when the tool exits non-zero.
pub(crate) fn audio_action(
    cx: &SessionCx<'_>,
    name: &str,
    value: Option<&Value>,
) -> Result<(), ApplyError> {
    let command: Vec<String> = match name {
        "audio.output.volume" => wpctl(
            "set-volume",
            "@DEFAULT_AUDIO_SINK@",
            py_float_argument(value)?,
        ),
        "audio.input.volume" => wpctl(
            "set-volume",
            "@DEFAULT_AUDIO_SOURCE@",
            py_float_argument(value)?,
        ),
        "audio.output.mute" => wpctl("set-mute", "@DEFAULT_AUDIO_SINK@", flag(value)),
        "audio.input.mute" => wpctl("set-mute", "@DEFAULT_AUDIO_SOURCE@", flag(value)),
        "audio.output.default" => pactl("set-default-sink", py_str(value)),
        "audio.input.default" => pactl("set-default-source", py_str(value)),
        // Unreachable: `action()` only routes a name in [`NAMES`] here, and this match covers
        // all six. Answered rather than panicked, for the same reason the workspace denies
        // `panic!`.
        other => return Err(ApplyError::Settings(format!("Unknown action: {other}"))),
    };
    let argv: Vec<&str> = command.iter().map(String::as_str).collect();
    run_checked(cx, &argv).map(drop)
}

fn wpctl(verb: &str, target: &str, argument: String) -> Vec<String> {
    vec![
        "wpctl".to_owned(),
        verb.to_owned(),
        target.to_owned(),
        argument,
    ]
}

fn pactl(verb: &str, argument: String) -> Vec<String> {
    vec!["pactl".to_owned(), verb.to_owned(), argument]
}

/// `"1" if value else "0"`: Python truthiness, so `0`, `""` and `[]` all unmute.
fn flag(value: Option<&Value>) -> String {
    if py_truthy(value) { "1" } else { "0" }.to_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::audio_action;
    use crate::testing::{Script, World};

    #[test]
    fn every_arm_reaches_the_tool_that_owns_the_concept() {
        let world = World::plain("audio", Script::new());
        world.with(|cx| {
            for (name, value) in [
                ("audio.output.volume", json!(0.65)),
                ("audio.input.volume", json!(1)),
                ("audio.output.mute", json!(true)),
                ("audio.input.mute", json!(0)),
                ("audio.output.default", json!("alsa_output.hdmi")),
                ("audio.input.default", json!("alsa_input.usb")),
            ] {
                audio_action(cx, name, Some(&value)).expect("the shim accepts every call");
            }
        });
        assert_eq!(
            world.trace(),
            [
                "wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.65",
                "wpctl set-volume @DEFAULT_AUDIO_SOURCE@ 1.0",
                "wpctl set-mute @DEFAULT_AUDIO_SINK@ 1",
                "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ 0",
                "pactl set-default-sink alsa_output.hdmi",
                "pactl set-default-source alsa_input.usb",
            ]
        );
    }

    #[test]
    fn a_refused_tool_is_reported_rather_than_swallowed() {
        let world = World::plain(
            "audio-refused",
            Script::new().answering("wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.5", 1, "", ""),
        );
        world.with(|cx| {
            let error = audio_action(cx, "audio.output.volume", Some(&json!(0.5)))
                .expect_err("wpctl refused");
            assert!(
                error
                    .to_string()
                    .starts_with("Command '['wpctl', 'set-volume'"),
                "{error}"
            );
        });
    }

    #[test]
    fn a_volume_payload_float_cannot_read_never_reaches_wpctl() {
        let world = World::plain("audio-bad-volume", Script::new());
        world.with(|cx| {
            let error = audio_action(cx, "audio.output.volume", Some(&json!("loud")))
                .expect_err("not a float");
            assert_eq!(
                error.to_string(),
                "could not convert string to float: 'loud'"
            );
        });
        assert!(world.trace().is_empty());
    }
}
