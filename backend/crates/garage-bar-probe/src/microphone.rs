//! The microphone half: whether any source is actually recording.
//!
//! Ported behaviour-exact from the waybar microphone module's `pactl list sources`
//! parser, because Quickshell's Pipewire service does not expose a node-level running
//! state on this install and the pulse layer is where that fact lives. A source counts
//! as recording when its state is `RUNNING` and its name does not end in `.monitor`
//! -- a monitor of an output is playback being tapped, not someone holding a mic.

use std::time::Duration;

use garage_core::process::{run, RunError};

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

/// Split into per-source sections and keep each description paired with its source.
fn parse(text: &str) -> Microphone {
    let recording_sources: Vec<Source> = text
        .split("Source #")
        .filter_map(source)
        .filter(Source::recording)
        .collect();
    Microphone {
        recording: !recording_sources.is_empty(),
        descriptions: recording_sources
            .into_iter()
            .filter_map(|source| source.description)
            .collect(),
    }
}

#[derive(Debug, Default)]
struct Source {
    name: String,
    state: String,
    description: Option<String>,
}

impl Source {
    fn recording(&self) -> bool {
        !self.name.ends_with(".monitor") && self.state == "RUNNING"
    }
}

fn source(section: &str) -> Option<Source> {
    let mut source = Source::default();
    for line in section.lines() {
        if let Some(name) = field(line, "Name:") {
            source.name = name;
        } else if let Some(state) = field(line, "State:") {
            source.state = state;
        } else if let Some(description) = field(line, "Description:") {
            source.description = Some(description);
        }
    }
    (!source.name.is_empty()).then_some(source)
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

    #[test]
    fn a_missing_description_cannot_shift_the_next_sources_label() {
        let sample = "Source #1\n State: RUNNING\n Name: first\n\
                      Source #2\n State: RUNNING\n Name: second\n Description: Second mic\n";
        let found = parse(sample);
        assert!(found.recording);
        assert_eq!(found.descriptions, ["Second mic"]);
    }
}
