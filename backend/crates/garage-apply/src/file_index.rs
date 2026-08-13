//! `apply_file_index()`: start, stop, or immediately refresh the background filename index.
//!
//! Disabling stops and disables the unit in one call and returns -- there is nothing further
//! to converge. Enabling is `enable` then `restart`, deliberately not `start`: frequency,
//! depth and roots are read at process startup, so a preference change has to build its first
//! new snapshot now rather than waiting for the old interval to expire, which only a restart
//! guarantees.
//!
//! Every step goes through the generic route runner ([`crate::route`]'s `run_or_raise()`
//! equivalent), so a `systemctl` failure at any of the three points is reported with a
//! message naming what specifically could not be done, rather than a bare non-zero exit.

use crate::cx::SessionCx;
use crate::error::ApplyError;
use crate::route::run_or_raise;

/// The unit the background filename index runs as.
const UNIT: &str = "garage-file-index.service";

/// Enable, disable or restart the background file index unit to match `[indexing]`
/// (garage:4842-4858).
///
/// # Errors
///
/// [`ApplyError::Signal`] naming what specifically could not be done -- "Unable to disable
/// background file indexing", "Unable to enable background file indexing" or "Unable to
/// refresh the background file index" -- or `systemctl`'s own complaint when it had one.
pub(crate) fn apply_file_index(cx: &mut SessionCx<'_>) -> Result<(), ApplyError> {
    if !cx.render().prefs().indexing.enabled {
        return run_or_raise(
            cx,
            &["systemctl", "--user", "disable", "--now", UNIT],
            "Unable to disable background file indexing",
        );
    }
    run_or_raise(
        cx,
        &["systemctl", "--user", "enable", UNIT],
        "Unable to enable background file indexing",
    )?;
    // Restart rather than start: frequency, depth and roots are read at process startup, and
    // a preference change should build its first new snapshot now instead of waiting for the
    // old interval to expire.
    run_or_raise(
        cx,
        &["systemctl", "--user", "restart", UNIT],
        "Unable to refresh the background file index",
    )
}

#[cfg(test)]
mod tests {
    use super::apply_file_index;
    use crate::testing::{Script, World};

    #[test]
    fn disabling_stops_and_disables_in_one_call_and_returns() {
        let world = World::new("index-off", "[indexing]\nenabled = false\n", Script::new());
        world.with(|cx| apply_file_index(cx).expect("the unit is disabled"));
        assert_eq!(
            world.signals(),
            ["systemctl --user disable --now garage-file-index.service"]
        );
    }

    #[test]
    fn enabling_is_enable_then_restart_never_start() {
        let world = World::new("index-on", "[indexing]\nenabled = true\n", Script::new());
        world.with(|cx| apply_file_index(cx).expect("the unit is enabled"));
        assert_eq!(
            world.signals(),
            [
                "systemctl --user enable garage-file-index.service",
                "systemctl --user restart garage-file-index.service",
            ]
        );
    }

    #[test]
    fn a_refused_step_names_what_could_not_be_done() {
        let world = World::new(
            "index-refused",
            "[indexing]\nenabled = true\n",
            Script::new().failing("systemctl --user enable"),
        );
        world.with(|cx| {
            let error = apply_file_index(cx).expect_err("systemctl refused");
            assert_eq!(
                error.to_string(),
                "Unable to enable background file indexing"
            );
        });
    }
}
