//! `render_region()`: publish the locale override and the bar's clock, as generated fragments.
//!
//! Both belong in files this crate cannot write directly: `~/.config/uwsm/env` and waybar's
//! `config.jsonc` are stow symlinks into the dotfiles repo, and rewriting either on a
//! settings change would edit a tracked file. So each one reaches in from its own side
//! instead -- the env file sources a generated `locale.env`, `config.jsonc` includes a
//! generated `waybar-clock.jsonc` -- and only the generated halves move.
//!
//! No export at all for an empty locale override, rather than exporting the system value:
//! the point of an empty override is that the session is left to resolve `LANG` the way it
//! would with the file absent, and exporting the resolved value would pin it instead.
//!
//! The bar clock is the one part of a locale choice that can be honoured before the next
//! login: its `std::locale` is handed in explicitly rather than read from the process
//! environment, so the weekday and month names move immediately rather than waiting for a
//! relaunch.

use garage_core::fs::atomic::atomic_write;
use garage_core::paths::Paths;
use garage_core::schema::enums::{DateFormat, FirstDayOfWeek, TimeFormat};
use garage_core::schema::Preferences;
use garage_core::shlex::shlex_quote;
use garage_core::toml_emit::{json_dumps, Value};

use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::template::shipped::{LOCALE_ENV, LOCALE_ENV_EXPORT, WAYBAR_CLOCK, WAYBAR_CLOCK_LOCALE};
use crate::template::vars::template_vars;
use crate::template::{NoVars, Template};

template_vars!(
    /// `locale.env`'s one line, already `shlex`-quoted: the file is a shell fragment, so
    /// the quoting is the renderer's and the `export` is the template's.
    LocaleExportVars { locale: String }
);

template_vars!(
    /// The bar clock's own four. `date_format` and `time_format` are handed over separately
    /// rather than joined here because the two spaces between them are text, and
    /// `locale_entry` is the expanded `waybar-clock-locale.tmpl` or nothing at all -- which
    /// key a JSON object carries is a condition, and a condition stays in Rust.
    ClockVars {
        date_format: &'static str,
        time_format: &'static str,
        iso8601: bool,
        locale_entry: String,
    }
);

template_vars!(
    /// The clock's `locale` member, already escaped as a JSON string body -- see
    /// [`json_body`].
    ClockLocaleVars { locale: String }
);

/// `DATE_FORMATS` (garage:320): the `strftime` date half, per `region.date_format`.
const fn date_format(format: DateFormat) -> &'static str {
    match format {
        DateFormat::Dmy => "%a %d %b",
        DateFormat::Mdy => "%a %b %d",
        DateFormat::Iso => "%a %Y-%m-%d",
    }
}

/// `TIME_FORMATS` (garage:319): the `strftime` time half, per `region.time_format`.
const fn time_format(format: TimeFormat) -> &'static str {
    match format {
        TimeFormat::Twelve => "%I:%M %p",
        TimeFormat::TwentyFour => "%H:%M",
    }
}

/// `FIRST_WEEKDAYS` (garage:323): waybar's calendar has no first-weekday option of its own,
/// only the ISO 8601 switch, which is defined as weeks beginning on Monday.
const fn first_weekday_is_monday(first_day: FirstDayOfWeek) -> bool {
    match first_day {
        FirstDayOfWeek::Sunday => false,
        FirstDayOfWeek::Monday => true,
    }
}

/// A string as JSON spells it, less the quotation marks the template carries.
///
/// The escaping is still `json.dumps`'s own -- [`json_dumps`] over a [`Value::Str`] is the
/// same encoder every other generated `.jsonc` goes through, so a locale with a quote or a
/// backslash in it lands in the file exactly as it did before this was a template. Only the
/// pair of quotes it wraps the result in comes back off, because in
/// `waybar-clock-locale.tmpl` they are part of the text.
fn json_body(text: &str) -> String {
    let quoted = json_dumps(&Value::Str(text.to_owned()), 2);
    quoted
        .strip_prefix('"')
        .and_then(|body| body.strip_suffix('"'))
        .unwrap_or(&quoted)
        .to_owned()
}

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

/// `waybar-clock.jsonc`'s whole body: the clock's `strftime` format wrapped for
/// `{fmt}`-style interpolation, the calendar's first-weekday switch, and the locale itself
/// when there is one to hand the formatter explicitly.
fn waybar_clock_json(paths: &Paths, prefs: &Preferences) -> Result<String, RenderError> {
    let region = &prefs.region;
    let locale = region.locale.as_str();
    let locale_entry = if locale.is_empty() {
        String::new()
    } else {
        Template::load(paths, WAYBAR_CLOCK_LOCALE).expand_block(&ClockLocaleVars {
            locale: json_body(locale),
        })?
    };
    let text = Template::load(paths, WAYBAR_CLOCK).expand(&ClockVars {
        date_format: date_format(region.date_format),
        time_format: time_format(region.time_format),
        iso8601: first_weekday_is_monday(region.first_day_of_week),
        locale_entry,
    })?;
    Ok(text)
}

/// Write the locale override and the bar's clock format (`render_region()`, garage:4525-4552).
///
/// # Errors
///
/// [`RenderError::Template`] if either fragment's template names a variable this renderer
/// does not supply, or [`RenderError::Atomic`] if either fragment could not be replaced.
pub fn render_region(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let paths = cx.paths();
    atomic_write(&paths.fragments.locale_env, &locale_env(paths, cx.prefs())?)?;
    atomic_write(
        &paths.fragments.waybar_clock,
        &waybar_clock_json(paths, cx.prefs())?,
    )?;
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

    /// `desktop/.local/bin/garage`'s own `render_region(FALLBACK_DEFAULTS)`, captured with
    /// `tests/harness.py` -- the shipped defaults carry no locale override.
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
        let clock = std::fs::read_to_string(&paths.fragments.waybar_clock)
            .expect("waybar-clock.jsonc was written");
        assert_eq!(
            clock,
            "{\n  \"clock\": {\n    \"format\": \"{:%a %d %b  %H:%M}\",\n    \"calendar\": {\n      \"iso8601\": false\n    }\n  }\n}\n"
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
        let clock = std::fs::read_to_string(&paths.fragments.waybar_clock)
            .expect("waybar-clock.jsonc was written");
        assert_eq!(
            clock,
            "{\n  \"clock\": {\n    \"format\": \"{:%a %Y-%m-%d  %I:%M %p}\",\n    \"calendar\": {\n      \"iso8601\": true\n    },\n    \"locale\": \"en_US.UTF-8\"\n  }\n}\n"
        );
        drop(std::fs::remove_dir_all(&paths.home));
    }
}
