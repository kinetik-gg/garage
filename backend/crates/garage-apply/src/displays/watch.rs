//! `watch()`: re-assert the saved layout whenever the display set settles.
//!
//! The watcher is a debounce around [`recover_display_layout()`](super::recover). It asks
//! the event source for the next display event with the quiet window as its timeout, so a
//! burst of hotplug lines -- a monitor powered off and on, several monitors in quick
//! succession -- collapses into one recovery once the wire goes quiet, rather than one per
//! line.
//!
//! # Best-effort, and honest about it
//!
//! A monitor that holds HPD asserted while powered off emits no event at all, on any
//! interface a userspace process can read: no `monitoradded`, no connector-status change,
//! only a kernel log line under one vendor's driver. Such a panel is invisible to this
//! watcher, and its black screen is only recoverable by hand or when some *other* monitor
//! in the same power cycle does drop HPD and drives the burst. That limit is why
//! `display-recover` is a command a person can run, and the watcher is a convenience on
//! top of it rather than the guarantee.

use std::time::Duration;

use garage_core::traits::MonitorEvents;

use crate::cx::SessionCx;
use crate::displays::recover::{recover_display_layout, Recovery};
use crate::error::ApplyError;

/// How long the display set must stay quiet before a burst is judged over.
const QUIET: Duration = Duration::from_secs(4);

/// Extra recovery passes after a burst, one quiet window apart. One sweep is enough to
/// catch a display that finishes powering on just after the last event; more would only
/// blink working panels longer.
const SWEEPS: u32 = 1;

/// Follow the display events, and put the saved layout back when they settle.
///
/// Runs until the event stream is gone, which is the only error it returns: a recovery
/// that fails is logged and the watch continues, because the next hotplug is a fresh
/// chance and a watcher that exited on one bad reload would be useless exactly when the
/// desktop is misbehaving.
///
/// # Errors
///
/// [`ApplyError::Io`] carrying the event source's own complaint when the stream ends.
pub fn watch(cx: &SessionCx<'_>, events: &mut dyn MonitorEvents) -> Result<(), ApplyError> {
    let mut dirty = false;
    let mut sweeps_left = 0_u32;
    loop {
        match events.next_event(QUIET) {
            Err(error) => return Err(ApplyError::Io(error.detail)),
            Ok(Some(_event)) => {
                dirty = true;
                sweeps_left = SWEEPS;
            }
            Ok(None) => {
                if dirty {
                    dirty = false;
                    recover(cx);
                } else if sweeps_left > 0 {
                    sweeps_left -= 1;
                    recover(cx);
                }
            }
        }
    }
}

/// One recovery, reported on stderr -- which is where a systemd user unit collects it.
fn recover(cx: &SessionCx<'_>) {
    match recover_display_layout(cx) {
        Ok(Recovery::Recovered { outputs }) => {
            eprintln!("garage display-watch: re-applied the saved layout to {outputs} display(s)");
        }
        Ok(Recovery::SkippedPending) => {
            eprintln!("garage display-watch: a display test is in flight; leaving it alone");
        }
        Ok(Recovery::NoLayout) => {}
        Err(error) => {
            eprintln!("garage display-watch: could not re-apply the layout: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::watch;
    use crate::testing::{Script, World};
    use garage_core::traits::{EventError, MonitorEvent, MonitorEvents};
    use std::time::Duration;

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

    /// A scripted stream: the events handed back in order, then a closed-stream error.
    struct Scripted {
        events: std::vec::IntoIter<Result<Option<MonitorEvent>, EventError>>,
    }

    impl Scripted {
        fn new(events: Vec<Result<Option<MonitorEvent>, EventError>>) -> Self {
            Self {
                events: events.into_iter(),
            }
        }
    }

    impl MonitorEvents for Scripted {
        fn next_event(&mut self, _timeout: Duration) -> Result<Option<MonitorEvent>, EventError> {
            self.events.next().unwrap_or_else(|| {
                Err(EventError {
                    detail: "closed".to_owned(),
                })
            })
        }
    }

    #[test]
    fn a_burst_collapses_into_a_recovery_on_the_quiet_edge() {
        let world = World::plain("watch", Script::new());
        write(&world.paths.host.displays, ONE_DISPLAY);
        let mut events = Scripted::new(vec![
            Ok(Some(MonitorEvent::Added("DP-1".to_owned()))),
            Ok(Some(MonitorEvent::Removed("DP-2".to_owned()))),
            Ok(None),
            Ok(None),
            Err(EventError {
                detail: "closed".to_owned(),
            }),
        ]);
        let result = world.with(|cx| watch(cx, &mut events));
        assert!(result.is_err(), "the closed stream ends the watch");
        let reloads = world
            .signals()
            .into_iter()
            .filter(|line| line == "hyprctl reload")
            .count();
        assert_eq!(reloads, 2, "one recovery plus one sweep");
    }

    #[test]
    fn a_quiet_stream_with_no_events_never_recovers() {
        let world = World::plain("watch-quiet", Script::new());
        write(&world.paths.host.displays, ONE_DISPLAY);
        let mut events = Scripted::new(vec![
            Ok(None),
            Ok(None),
            Ok(None),
            Err(EventError {
                detail: "closed".to_owned(),
            }),
        ]);
        let result = world.with(|cx| watch(cx, &mut events));
        assert!(result.is_err());
        assert!(
            world.signals().is_empty(),
            "no event means no recovery: {:?}",
            world.signals()
        );
    }
}
