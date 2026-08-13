//! `hyprctl monitors -j`, parsed down to the three fields a render reads.

use garage_core::traits::{Monitor, MonitorError, MonitorSource, Runner, DEFAULT_RUN_TIMEOUT};
use serde_json::Value;

/// The compositor, asked the one question a render may ask it.
///
/// The Python reaches this through `json_command(["hyprctl", "monitors", "-j"], [])`, which
/// folds four different failures -- missing binary, timeout, non-zero exit, unparseable
/// output -- into the same empty list. [`MonitorSource`] keeps them as an `Err`, and a
/// caller that wants the Python's reading writes `unwrap_or_default()`; see the trait's own
/// docs for why the line is drawn there rather than here.
#[derive(Clone, Copy)]
pub struct Hyprctl<'a> {
    runner: &'a dyn Runner,
}

/// Hand-written for the same reason as [`crate::Luac`]'s: the field is a trait object.
impl std::fmt::Debug for Hyprctl<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Hyprctl").finish_non_exhaustive()
    }
}

impl<'a> Hyprctl<'a> {
    /// Wrap a runner.
    #[must_use]
    pub const fn new(runner: &'a dyn Runner) -> Self {
        Self { runner }
    }
}

impl MonitorSource for Hyprctl<'_> {
    /// # Errors
    ///
    /// [`MonitorError`] when `hyprctl` cannot be run, exits non-zero, or prints something
    /// that is not a JSON array. A compositor that answers `[]` is `Ok(vec![])`.
    fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
        let output = self
            .runner
            .run(&["hyprctl", "monitors", "-j"], DEFAULT_RUN_TIMEOUT)
            .map_err(|error| MonitorError {
                detail: error.detail,
            })?;
        if output.status != 0 {
            return Err(MonitorError {
                detail: format!("hyprctl monitors -j exited {}", output.status),
            });
        }
        let document: Value =
            serde_json::from_str(&output.stdout).map_err(|error| MonitorError {
                detail: format!("hyprctl monitors -j printed something unreadable: {error}"),
            })?;
        let Some(items) = document.as_array() else {
            return Err(MonitorError {
                detail: "hyprctl monitors -j did not answer with a list".to_owned(),
            });
        };
        Ok(items.iter().filter_map(monitor).collect())
    }
}

/// One record, or `None` for an entry that is not an object at all.
///
/// A record with no usable `name` is kept rather than dropped, with the empty string the
/// Python's `item.get("name", "")` would produce. The two callers disagree about it and
/// both are served by keeping it: `monitor_names()` filters falsy names out itself, while
/// `solid_wallpaper()` maximises over *every* record's width and height and would size the
/// flat PNG differently if one were missing here.
fn monitor(item: &Value) -> Option<Monitor> {
    let record = item.as_object()?;
    Some(Monitor {
        name: record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        width: pixels(record.get("width")),
        height: pixels(record.get("height")),
    })
}

/// `int(item.get("width", 0))`, with anything that is not a usable number reading as zero --
/// which is the value callers already treat as "reported nothing usable".
fn pixels(value: Option<&Value>) -> u32 {
    value
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;
    use std::time::Duration;

    use garage_core::traits::{MonitorSource as _, Output, RunError, Runner};

    use super::Hyprctl;

    struct Says(&'static str, i32);

    impl Runner for Says {
        fn run(&self, _command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
            Ok(Output {
                status: self.1,
                stdout: self.0.to_owned(),
                stderr: String::new(),
            })
        }

        fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
            Ok(())
        }

        fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
            Ok(0)
        }
    }

    struct Absent(Cell<bool>);

    impl Runner for Absent {
        fn run(&self, _command: &[&str], _timeout: Duration) -> Result<Output, RunError> {
            self.0.set(true);
            Err(RunError {
                detail: "[Errno 2] No such file or directory: 'hyprctl'".to_owned(),
            })
        }

        fn spawn_detached(&self, _command: &[&str]) -> Result<(), RunError> {
            Ok(())
        }

        fn run_streamed(&self, _command: &[&str], _cwd: Option<&Path>) -> Result<i32, RunError> {
            Ok(0)
        }
    }

    #[test]
    fn two_displays_come_back_with_their_names_and_sizes() {
        let runner = Says(
            r#"[{"name":"DP-1","width":3440,"height":1440},
                {"name":"eDP-1","width":1920,"height":1080}]"#,
            0,
        );
        let seen = Hyprctl::new(&runner).monitors().expect("the fake answers");
        assert_eq!(seen.len(), 2);
        assert_eq!(seen.first().map(|first| first.name.as_str()), Some("DP-1"));
        assert_eq!(seen.first().map(|first| first.width), Some(3440));
        assert_eq!(seen.get(1).map(|second| second.height), Some(1080));
    }

    #[test]
    fn a_record_with_no_name_is_kept_at_the_empty_string() {
        let runner = Says(r#"[{"width":800,"height":600}]"#, 0);
        let seen = Hyprctl::new(&runner).monitors().expect("the fake answers");
        assert_eq!(seen.first().map(|first| first.name.as_str()), Some(""));
        assert_eq!(seen.first().map(|first| first.width), Some(800));
    }

    #[test]
    fn a_missing_size_reads_as_zero_rather_than_failing_the_question() {
        let runner = Says(r#"[{"name":"DP-1","width":"wide"}]"#, 0);
        let seen = Hyprctl::new(&runner).monitors().expect("the fake answers");
        assert_eq!(seen.first().map(|first| first.width), Some(0));
    }

    #[test]
    fn no_displays_attached_is_an_empty_list_and_not_an_error() {
        let runner = Says("[]", 0);
        assert!(Hyprctl::new(&runner)
            .monitors()
            .expect("the fake answers")
            .is_empty());
    }

    #[test]
    fn every_way_of_not_answering_is_an_error_the_caller_may_ignore() {
        for runner in [Says("not json", 0), Says("{}", 0), Says("[]", 1)] {
            assert!(Hyprctl::new(&runner).monitors().is_err());
        }
        let absent = Absent(Cell::new(false));
        assert!(Hyprctl::new(&absent).monitors().is_err());
        assert!(absent.0.get());
        // The Python's reading of all four, in the one line it costs.
        let broken = Says("not json", 0);
        assert!(Hyprctl::new(&broken)
            .monitors()
            .unwrap_or_default()
            .is_empty());
    }
}
