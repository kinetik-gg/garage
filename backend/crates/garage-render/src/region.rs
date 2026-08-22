//! `render_region()`: publish the locale override and the bar's clock format.
//!
//! The locale lands in `locale.env`, which `~/.config/uwsm/env` sources -- a stow symlink,
//! so rewriting it on a settings change would edit a tracked file; only the generated half
//! moves. No export at all for an empty override, rather than exporting the system value:
//! the point of an empty override is that the session is left to resolve `LANG` the way it
//! would with the file absent, and exporting the resolved value would pin it instead.
//!
//! The clock format lands in the watched `clock-format.json` marker, which is what makes
//! the bar clock the one part of a locale choice honoured before the next login.

use garage_core::fs::atomic::atomic_write;
use garage_core::fs::marker::write_marker;
use garage_core::paths::Paths;
use garage_core::schema::Preferences;
use garage_core::shlex::shlex_quote;
use garage_core::toml_emit::{json_dumps, Value};

use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::template::shipped::{LOCALE_ENV, LOCALE_ENV_EXPORT};
use crate::template::vars::template_vars;
use crate::template::{NoVars, Template};

template_vars!(
    /// `locale.env`'s one line, already `shlex`-quoted: the file is a shell fragment, so
    /// the quoting is the renderer's and the `export` is the template's.
    LocaleExportVars { locale: String }
);

/// `locale.env`'s whole body: the header comment, then an `export LANG=...` line only when
/// the override is not empty.
fn locale_env(paths: &Paths, prefs: &Preferences) -> Result<String, RenderError> {
    let mut text = Template::load(paths, LOCALE_ENV).expand(&NoVars)?;
    let locale = prefs.region.locale.as_str();
    if !locale.is_empty() {
        text.push_str(
            &Template::load(paths, LOCALE_ENV_EXPORT).expand(&LocaleExportVars {
                locale: shlex_quote(locale),
            })?,
        );
    }
    Ok(text)
}

/// `clock-format.json`'s whole body: the four region values the Quickshell bar's clock
/// consumes, as the schema spells them. The locale is carried even when empty -- an empty
/// string is "resolve it the way the session would", which is a state worth publishing,
/// not an absence to infer.
fn clock_format_json(prefs: &Preferences) -> String {
    let region = &prefs.region;
    let value = Value::Table(vec![
        (
            "locale".to_owned(),
            Value::Str(region.locale.as_str().to_owned()),
        ),
        (
            "date_format".to_owned(),
            Value::Str(region.date_format.as_str().to_owned()),
        ),
        (
            "time_format".to_owned(),
            Value::Str(region.time_format.as_str().to_owned()),
        ),
        (
            "first_day_of_week".to_owned(),
            Value::Str(region.first_day_of_week.as_str().to_owned()),
        ),
    ]);
    format!("{}\n", json_dumps(&value, 2))
}

/// Write the locale override and the bar's clock format (`render_region()`, garage:4525-4552).
///
/// # Errors
///
/// [`RenderError::Template`] if either fragment's template names a variable this renderer
/// does not supply, [`RenderError::Atomic`] if either fragment could not be replaced, or
/// [`RenderError::Marker`] if the clock-format marker could not be written.
pub fn render_region(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let paths = cx.paths();
    atomic_write(&paths.fragments.locale_env, &locale_env(paths, cx.prefs())?)?;
    write_marker(&paths.markers.clock_format, &clock_format_json(cx.prefs()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use garage_core::paths::Paths;
    use garage_core::schema::defaults::Defaults;
    use garage_core::schema::notes::Notes;
    use garage_core::schema::Preferences;
    use garage_core::traits::{
        LuaCheckError, LuaSyntaxCheck, Monitor, MonitorError, MonitorSource,
    };

    use super::render_region;
    use crate::cx::RenderCx;

    fn prefs_from(departures: &str) -> Preferences {
        let table: toml::Table = departures.parse().expect("fixture toml parses");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let mut notes = Notes::new();
        Preferences::coerce_from(&table, &defaults, &mut notes)
    }

    struct NoMonitors;
    impl MonitorSource for NoMonitors {
        fn monitors(&self) -> Result<Vec<Monitor>, MonitorError> {
            Ok(vec![])
        }
    }

    struct LuaAccepts;
    impl LuaSyntaxCheck for LuaAccepts {
        fn check(&self, _candidate: &Path) -> Result<(), LuaCheckError> {
            Ok(())
        }
    }

    fn scratch_paths(label: &str) -> Paths {
        let home = std::env::temp_dir().join(format!(
            "garage-render-region-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let env: HashMap<String, String> =
            [("HOME".to_owned(), home.to_string_lossy().into_owned())]
                .into_iter()
                .collect();
        Paths::from_env_map(&env)
    }

    /// The former backend's `render_region(FALLBACK_DEFAULTS)` output, captured during the
    /// Rust port -- the shipped defaults carry no locale override.
    #[test]
    fn matches_the_python_backend_with_no_locale_override() {
        let prefs = prefs_from("");
        let paths = scratch_paths("defaults");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);
        render_region(&cx).expect("render_region succeeds on a clean scratch");

        let locale_env =
            std::fs::read_to_string(&paths.fragments.locale_env).expect("locale.env was written");
        assert_eq!(
            locale_env,
            "# Generated by garage. Sourced from ~/.config/uwsm/env.\n"
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }

    /// A locale override, an ISO date, a 12-hour clock and a Monday-first calendar --
    /// captured the same way, so the export line, the format string and the `locale` key
    /// added to the clock object are all pinned against the real backend.
    #[test]
    fn matches_the_python_backend_with_a_locale_override() {
        let prefs = prefs_from(
            "[region]\nlocale = \"en_US.UTF-8\"\ndate_format = \"iso\"\ntime_format = \"12\"\n\
             first_day_of_week = \"monday\"\n",
        );
        let paths = scratch_paths("locale");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);
        render_region(&cx).expect("render_region succeeds on a clean scratch");

        let locale_env =
            std::fs::read_to_string(&paths.fragments.locale_env).expect("locale.env was written");
        assert_eq!(
            locale_env,
            "# Generated by garage. Sourced from ~/.config/uwsm/env.\nexport LANG=en_US.UTF-8\n"
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }

    /// The Quickshell bar's clock marker carries the same four values the waybar fragment
    /// spells as `strftime`, in the schema's own words, with an empty locale published as
    /// a state rather than omitted.
    #[test]
    fn the_clock_format_marker_carries_the_region_values() {
        let prefs = prefs_from(
            "[region]\nlocale = \"en_US.UTF-8\"\ndate_format = \"iso\"\ntime_format = \"12\"\n\
             first_day_of_week = \"monday\"\n",
        );
        let paths = scratch_paths("marker");
        let monitors = NoMonitors;
        let lua = LuaAccepts;
        let cx = RenderCx::new(&prefs, &paths, &monitors, &lua);
        render_region(&cx).expect("render_region succeeds on a clean scratch");

        let marker = std::fs::read_to_string(&paths.markers.clock_format)
            .expect("the clock-format marker was written");
        assert_eq!(
            marker,
            concat!(
                "{\n",
                "  \"locale\": \"en_US.UTF-8\",\n",
                "  \"date_format\": \"iso\",\n",
                "  \"time_format\": \"12\",\n",
                "  \"first_day_of_week\": \"monday\"\n",
                "}\n"
            )
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
