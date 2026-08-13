//! `microphone()`: is anything actually recording right now.
//!
//! `pactl list sources` prints one `Source #N` section per source; the Python splits
//! on the lookahead regex `(?=^Source #)` (`re.MULTILINE`) and then, within each
//! section, `re.search`es for the first `Name:`/`State:`/`Description:` line. No
//! `regex` crate is in this workspace, so [`sections`] and [`field`] below reproduce
//! that splitting and searching by hand rather than adding one for three patterns
//! this simple.

use std::time::Duration;

use crate::context::run::run_ignoring_status;
use crate::context::theme;
use crate::exec::RunError;
use crate::waybar::Payload;
use garage_core::paths::Paths;

const TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) fn payload(paths: &Paths) -> Result<Payload, RunError> {
    let output = run_ignoring_status(&["pactl", "list", "sources"], TIMEOUT)?;
    let recording = recording_sources(&output);
    Ok(if recording.is_empty() {
        idle_payload(paths)
    } else {
        recording_payload(&recording)
    })
}

/// Split `output` the way `re.split(r"(?=^Source #)", output, flags=re.MULTILINE)`
/// does: a lookahead split keeps the matched delimiter as the start of the next
/// piece, so the text before the very first `Source #` line (if any) is its own
/// leading section, and every `Source #` line begins a new one.
fn sections(output: &str) -> Vec<Vec<&str>> {
    let mut sections: Vec<Vec<&str>> = vec![Vec::new()];
    for line in output.lines() {
        if line.starts_with("Source #") {
            sections.push(Vec::new());
        }
        if let Some(current) = sections.last_mut() {
            current.push(line);
        }
    }
    sections
}

/// `re.search(rf"^\s*{label}\s*(.+)$", section, re.MULTILINE)`'s capture group, for
/// the first matching line in `section`. `label` includes the trailing colon (e.g.
/// `"Name:"`), matching the Python patterns' `Name:`/`State:`/`Description:` literals.
fn field<'a>(section: &[&'a str], label: &str) -> Option<&'a str> {
    section.iter().find_map(|line| {
        let value = line.trim_start().strip_prefix(label)?.trim_start();
        (!value.is_empty()).then_some(value)
    })
}

fn recording_sources(output: &str) -> Vec<String> {
    sections(output)
        .into_iter()
        .filter_map(|section| recording_label(&section))
        .collect()
}

/// `if name and state and state.group(1) == "RUNNING" and not
/// name.group(1).endswith(".monitor")`, returning the description when present, else
/// the raw source name.
fn recording_label(section: &[&str]) -> Option<String> {
    let name = field(section, "Name:")?;
    let state = field(section, "State:")?;
    if state != "RUNNING" || name.ends_with(".monitor") {
        return None;
    }
    Some(field(section, "Description:").unwrap_or(name).to_string())
}

fn recording_payload(recording: &[String]) -> Payload {
    Payload {
        text: "MIC <span foreground=\"#ffd60ae6\">\u{25cf}</span>".to_string(),
        tooltip: format!("Microphone active\n{}", recording.join("\n")),
        class: "recording".to_string(),
    }
}

fn idle_payload(paths: &Paths) -> Payload {
    let theme_fg = theme::foreground(paths);
    Payload {
        text: format!("MIC <span foreground=\"{theme_fg}73\">\u{25cf}</span>"),
        tooltip: "Microphone inactive".to_string(),
        class: "idle".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::recording_sources;

    const SAMPLE: &str = "\
Source #0
\tState: SUSPENDED
\tName: alsa_output.pci.analog-stereo.monitor
\tDescription: Built-in Audio Analog Stereo Monitor
Source #1
\tState: RUNNING
\tName: alsa_input.usb.mono-fallback
\tDescription: USB Microphone
Source #2
\tState: IDLE
\tName: alsa_input.pci.analog-stereo
\tDescription: Built-in Microphone
";

    #[test]
    fn only_running_non_monitor_sources_are_reported() {
        assert_eq!(
            vec!["USB Microphone".to_string()],
            recording_sources(SAMPLE)
        );
    }

    #[test]
    fn a_running_monitor_source_is_excluded() {
        let monitor_running = "Source #0\n\tState: RUNNING\n\tName: alsa.monitor\n";
        assert!(recording_sources(monitor_running).is_empty());
    }

    #[test]
    fn falls_back_to_the_raw_name_with_no_description_line() {
        let no_description = "Source #0\n\tState: RUNNING\n\tName: raw-source-name\n";
        assert_eq!(
            vec!["raw-source-name".to_string()],
            recording_sources(no_description)
        );
    }

    #[test]
    fn no_output_at_all_reports_nothing_recording() {
        assert!(recording_sources("").is_empty());
    }
}
