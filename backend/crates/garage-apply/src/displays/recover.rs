//! `recover_display_layout()`: put the saved layout back on screen after a hotplug burst.
//!
//! # Why this exists
//!
//! A monitor powered off and on again does not always come back. The compositor can still
//! report the output `enabled` with DPMS on while the physical link never re-trains, which
//! leaves a black panel and no event anywhere in userspace to say so. The one signal that
//! is reliable -- on some panels, and only some -- is the hotplug event itself, and even
//! that is absent on a monitor that holds HPD asserted while switched off. So recovery is
//! deliberately *defensive*: whenever the display set settles, put the saved layout back,
//! whether or not anything was observably broken.
//!
//! # What it does
//!
//! Re-applying the layout is the same operation the Displays pane's confirm already ships:
//! [`apply_display_layout()`](super::apply::apply_display_layout) checks the geometry,
//! renders the fragment and reloads the compositor. That alone may not re-train a dead
//! link, because Hyprland has no reason to tear down an output whose rule has not changed,
//! so each enabled display is then power-cycled at the sink with DPMS off/on -- the
//! closest software analogue to the monitor power-button press that fixes the panel by
//! hand. Neither half is proven against a real black screen yet; that validation is what
//! `garage display-recover` exists to make possible.
//!
//! # What it must never do
//!
//! It reads `displays.toml` and never writes it: the two-writer rule (`display_finish()`
//! and `initialize_display_config()`) is untouched, and a recovery cannot lose a user's
//! layout. It takes `DISPLAY_LOCK` so it cannot race a layout transaction, and it stands
//! down entirely while one is pending -- an in-flight `display-test` is between layouts on
//! purpose, and healing it back to the saved one would fight the user's test.

use std::thread;
use std::time::Duration;

use garage_render::displays::{load_display_config, DisplayEntry};

use crate::command::run;
use crate::cx::SessionCx;
use crate::displays::apply::apply_display_layout;
use crate::displays::transaction::DisplayLock;
use crate::error::ApplyError;

/// How long a display is held off during its re-lock, before it is switched back on. Long
/// enough that the sink sees a power transition rather than a blink.
const RELOCK_PAUSE: Duration = Duration::from_millis(250);

/// What [`recover_display_layout`] did, for the caller to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// The machine has no saved layout, so the catch-all rule is already the layout and
    /// there is nothing to put back.
    NoLayout,
    /// A display test is in flight; recovery stood down rather than fight it.
    SkippedPending,
    /// The saved layout was put back and `outputs` displays were re-locked.
    Recovered {
        /// How many enabled displays the saved layout names.
        outputs: usize,
    },
}

/// Put the saved display layout back, and re-lock every enabled output.
///
/// # Errors
///
/// [`ApplyError::Render`] if `displays.toml` cannot be read, [`ApplyError::DisplayLock`] if
/// the display lock cannot be taken, and whatever
/// [`apply_display_layout()`](super::apply::apply_display_layout) refuses. A re-lock
/// dispatch that fails is not reported: it is best-effort insurance on top of the reload,
/// and a compositor that refused it has already said so through the reload.
pub fn recover_display_layout(cx: &SessionCx<'_>) -> Result<Recovery, ApplyError> {
    let paths = cx.render().paths();
    let layout = load_display_config(&paths.host.displays)?;
    if layout.displays.is_empty() {
        return Ok(Recovery::NoLayout);
    }
    let lock = DisplayLock::acquire(&paths.locks.display)?;
    if paths.pending_display.exists() {
        return Ok(Recovery::SkippedPending);
    }
    let outputs: Vec<String> = layout
        .displays
        .iter()
        .filter(|entry| entry.enabled())
        .map(DisplayEntry::output)
        .collect();
    apply_display_layout(cx, &layout)?;
    for output in &outputs {
        relock(cx, output);
    }
    drop(lock);
    Ok(Recovery::Recovered {
        outputs: outputs.len(),
    })
}

/// Power-cycle one output at the sink: `DPMS off`, a pause, `DPMS on`.
///
/// The expression is built here rather than taken from input, and an output name that is
/// not a plain connector token is skipped outright, so nothing saved in `displays.toml`
/// can become syntax inside the dispatch call.
fn relock(cx: &SessionCx<'_>, output: &str) {
    if !is_connector_token(output) {
        return;
    }
    for mode in ["off", "on"] {
        let expression = format!("hl.dsp.dpms({{ mode = \"{mode}\", monitor = \"{output}\" }})");
        drop(run(cx, &["hyprctl", "dispatch", expression.as_str()]));
        thread::sleep(RELOCK_PAUSE);
    }
}

/// Whether a name is safe to embed in a dispatch expression: connector names are letters,
/// digits, `-` and `_`, and nothing else is allowed through.
fn is_connector_token(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

#[cfg(test)]
mod tests {
    use super::{is_connector_token, recover_display_layout, Recovery};
    use crate::testing::{Script, World};

    const ONE_DISPLAY: &str = r#"
primary = "DP-1"

[[display]]
output = "DP-1"
enabled = true
mode = "1920x1080@60"
x = 0
y = 0
scale = 1
"#;

    /// Write a scratch file, creating the directory the world's path arithmetic names.
    fn write(path: &std::path::Path, body: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("scratch parent");
        }
        std::fs::write(path, body).expect("scratch file");
    }

    #[test]
    fn a_saved_layout_is_reapplied_and_its_output_is_relocked() {
        let world = World::plain("recover", Script::new());
        write(&world.paths.host.displays, ONE_DISPLAY);
        let outcome = world.with(|cx| recover_display_layout(cx).expect("recovery succeeds"));
        assert_eq!(outcome, Recovery::Recovered { outputs: 1 });
        let signals = world.signals();
        assert!(
            signals.iter().any(|line| line == "hyprctl reload"),
            "the layout is reloaded: {signals:?}"
        );
        assert!(
            signals
                .iter()
                .any(|line| line.contains("hl.dsp.dpms") && line.contains("\"off\"")),
            "the output is switched off: {signals:?}"
        );
        assert!(
            signals
                .iter()
                .any(|line| line.contains("hl.dsp.dpms") && line.contains("\"on\"")),
            "the output is switched back on: {signals:?}"
        );
    }

    #[test]
    fn no_saved_layout_is_a_no_op() {
        let world = World::plain("recover-none", Script::new());
        let outcome = world.with(|cx| recover_display_layout(cx).expect("recovery succeeds"));
        assert_eq!(outcome, Recovery::NoLayout);
        assert!(world.signals().is_empty(), "nothing is signalled");
    }

    #[test]
    fn a_pending_display_test_is_left_alone() {
        let world = World::plain("recover-pending", Script::new());
        write(&world.paths.host.displays, ONE_DISPLAY);
        write(&world.paths.pending_display, "{}");
        let outcome = world.with(|cx| recover_display_layout(cx).expect("recovery succeeds"));
        assert_eq!(outcome, Recovery::SkippedPending);
        assert!(world.signals().is_empty(), "nothing is signalled");
    }

    #[test]
    fn only_plain_connector_names_reach_the_dispatch() {
        assert!(is_connector_token("DP-1"));
        assert!(is_connector_token("eDP-1"));
        assert!(is_connector_token("HDMI_A-2"));
        assert!(!is_connector_token(""));
        assert!(!is_connector_token("DP-1\") ; do_something("));
        assert!(!is_connector_token("DP 1"));
    }
}
