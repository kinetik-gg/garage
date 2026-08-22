//! The microphone half: whether any source is actually recording.
//!
//! Ported behaviour-exact from the waybar microphone module's `pactl list sources`
//! parser, because Quickshell's Pipewire service does not expose a node-level running
//! state on this install and the pulse layer is where that fact lives. A source counts
//! as recording when its state is `RUNNING` and its name does not end in `.monitor`
//! -- a monitor of an output is playback being tapped, not someone holding a mic.

use std::time::Duration;

use crate::exec::{run, RunError};

const TIMEOUT: Duration = Duration::from_secs(3);

/// Whether anything is recording, plus the descriptions worth showing in a tooltip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Microphone {
    pub(crate) recording: bool,
    pub(crate) descriptions: Vec<String>,
}

pub(crate) fn probe() -> Result<Microphone, RunError> {
    let output = run(&["pactl", "list", "sources"], TIMEOUT)?;
    Ok(parse(&output.stdout))
}

/// Split into per-source sections and read `Name:` / `State:` / `Description:`.
fn parse(text: &str) -> Microphone {
    let mut names = Vec::<String>::new();
    let mut states = Vec::<String>::new();
    let mut descriptions = Vec::<String>::new();
    for line in text.lines() {
        if let Some(name) = field(line, "Name:") {
            names.push(name);
        } else if let Some(state) = field(line, "State:") {
            states.push(state);
        } else if let Some(description) = field(line, "Description:") {
            descriptions.push(description);
        }
    }
    let recording_sources: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(index, name)| {
            !name.ends_with(".monitor")
                && states.get(*index).is_some_and(|state| state == "RUNNING")
        })
        .map(|(index, _)| index)
        .collect();
    Microphone {
        recording: !recording_sources.is_empty(),
        descriptions: recording_sources
            .iter()
            .filter_map(|index| descriptions.get(*index).cloned())
            .collect(),
    }
}

fn field(line: &str, key: &str) -> Option<String> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix(key)
        .map(|rest| rest.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{field, parse};

    const SAMPLE: &str = "Source #57\n\tState: SUSPENDED\n\tName: alsa_output.monitor\n\
        \tDescription: Built-in Audio Analog Stereo (monitor)\nSource #62\n\tState: RUNNING\n\
        \tName: alsa_input.usb_mic\n\tDescription: USB Microphone\n";

    #[test]
    fn a_running_non_monitor_source_is_recording() {
        let found = parse(SAMPLE);
        assert!(found.recording);
        assert_eq!(vec!["USB Microphone".to_string()], found.descriptions);
    }

    #[test]
    fn monitors_never_count_even_when_running() {
        let only_monitor = SAMPLE.replace(
            "State: RUNNING\n\tName: alsa_input.usb_mic",
            "State: RUNNING\n\tName: alsa_output.monitor",
        );
        assert!(!parse(&only_monitor).recording);
    }

    #[test]
    fn suspended_sources_do_not_count() {
        let quiet = SAMPLE.replace("RUNNING", "SUSPENDED");
        assert!(!parse(&quiet).recording);
    }

    #[test]
    fn fields_are_trimmed_of_their_key_and_whitespace() {
        assert_eq!(
            Some("RUNNING".to_owned()),
            field("\t\tState:   RUNNING ", "State:")
        );
        assert_eq!(None, field("", "State:"));
    }
}
