//! `resolve_theme()`, `applied_scheme()` and `render_theme()`: which palette is in effect,
//! which one is live, and writing the one that is in effect.
//!
//! `resolve_theme()` is the schedule: `light`, `dark`, or `auto`, which reads as a light
//! window between two times of day, wrapping midnight the same way the night shift schedule
//! does. `applied_scheme()` is a different question entirely, and the distinction is
//! load-bearing -- it is written by `push_theme()` and nothing else, in particular not by
//! `render_theme()`, so it is by definition the scheme currently live on the desktop rather
//! than the scheme last generated. That is what makes it usable as the change gate:
//! `ApplyStep::ThemeIfSchemeMoved` compares `resolve_theme()` against `applied_scheme()`
//! rather than against anything a render wrote, because a render that wrote the files
//! without pushing them must still leave `applied_scheme()` saying what is on screen.
//!
//! `render_theme()` itself writes every file the resolved palette decides, and is pure --
//! nothing is signalled. Only generated paths are written; everything under `~/.config` this
//! checkout tracks is a stow symlink into the dotfiles repo, so writing there would edit
//! tracked files and leave the working tree dirty on every switch. `SCHEME_FILE` is
//! deliberately *not* written from here, for the same reason `applied_scheme()` reads it
//! rather than a render output: writing it from a pure render would make the change gate
//! think the apply it is guarding had already happened.
//!
//! `render_theme()` calls [`crate::search::render_search_engine`] and the toolkit writers in
//! [`crate::palette`] as part of its own body -- the launcher marker and every toolkit config
//! have to stay current across a theme switch even when neither the search engine nor any
//! individual toolkit setting moved.
//!
//! # The clock, and where it is read
//!
//! Only the `auto` arm needs a clock, and only at minute resolution against a *local* time of
//! day -- the Python's `time.localtime()`, which is libc's, which is the machine's timezone.
//! The comparison itself is pure integer arithmetic, so [`resolve_theme_at`] and
//! [`night_shift_active_at`] take `now_minutes` as an argument and every test drives them
//! from a table. [`local_minutes_of_day`] is the whole of the impure half: one function, one
//! call site each in [`resolve_theme`] and [`night_shift_active`].
//!
//! `std` has no local time. The three ways out were a hand-written `TZif` reader, `chrono`'s
//! `clock` feature, and a small dedicated timezone crate; this crate takes the third,
//! `tz-rs`. A hand-written reader is not the minimal answer it looks like: modern `tzdata` is
//! built slim, so `/etc/localtime`'s transition table stops at the last *historical* rule
//! change and every future instant is decided by the POSIX `TZ` string in the file's footer
//! -- a correct reader has to parse and evaluate that footer's DST rules too, which is a few
//! hundred lines of exactly the code `tz-rs` already is. `chrono` is correct as well but
//! costs four lock entries (`chrono`, `num-traits`, `iana-time-zone`, `autocfg`) for the one
//! call below. `tz-rs` is pure Rust, carries `#![forbid(unsafe_code)]` itself, honours `TZ`
//! ahead of `/etc/localtime` the way libc does, and has no dependencies at all: one lock
//! entry. No `unsafe` is introduced on either side of the boundary -- this crate forbids it,
//! and so does the crate it now calls.

use garage_core::paths::Paths;
use garage_core::pyrepr::py_str_repr;
use garage_core::schema::enums::{Scheme, ThemeMode};
use garage_core::schema::Preferences;

use crate::cx::RenderCx;
use crate::error::RenderError;
use crate::palette::table::role as palette_role;
use crate::palette::toolkits::render_toolkits;
use crate::search::render_search_engine;

/// Write every file the resolved palette decides: the toolkit configs and the search engine
/// marker (garage:4628-4652).
///
/// # Errors
///
/// [`RenderError::Marker`] if the search engine marker could not be written, or whatever
/// [`render_toolkits`] returns.
pub fn render_theme(cx: &RenderCx<'_>) -> Result<(), RenderError> {
    let scheme = resolve_theme(cx.prefs());
    render_search_engine(cx)?;
    render_toolkits(cx.paths(), scheme)
}

/// Which palette is in effect right now (garage:3693-3711), reading the wall clock through
/// [`local_minutes_of_day`].
#[must_use]
pub fn resolve_theme(prefs: &Preferences) -> Scheme {
    resolve_theme_at(prefs, local_minutes_of_day())
}

/// [`resolve_theme`] with the clock supplied, which is the whole of the decision.
///
/// Auto reads as a light window between the two times, wrapping midnight the same way the
/// night shift schedule does. `light == dark` is not a window at all, and the Python answers
/// `dark` for it rather than picking a side of a zero-width interval.
#[must_use]
pub fn resolve_theme_at(prefs: &Preferences, now_minutes: u16) -> Scheme {
    let appearance = &prefs.appearance;
    match appearance.theme_mode {
        ThemeMode::Light => return Scheme::Light,
        ThemeMode::Dark => return Scheme::Dark,
        ThemeMode::Auto => {}
    }
    let light = appearance.theme_light_at.minutes_from_midnight();
    let dark = appearance.theme_dark_at.minutes_from_midnight();
    if light == dark {
        return Scheme::Dark;
    }
    let daytime = if light < dark {
        light <= now_minutes && now_minutes < dark
    } else {
        now_minutes >= light || now_minutes < dark
    };
    if daytime {
        Scheme::Light
    } else {
        Scheme::Dark
    }
}

/// Whether the night shift schedule covers this moment (garage:3682-3690), reading the wall
/// clock through [`local_minutes_of_day`].
#[must_use]
pub fn night_shift_active(prefs: &Preferences) -> bool {
    night_shift_active_at(prefs, local_minutes_of_day())
}

/// [`night_shift_active`] with the clock supplied.
///
/// A `start == end` schedule falls into the wrapping branch, where `current >= start ||
/// current < end` is true for every minute of the day. That is the Python's answer as
/// written, and it is preserved rather than special-cased.
#[must_use]
pub fn night_shift_active_at(prefs: &Preferences, now_minutes: u16) -> bool {
    let appearance = &prefs.appearance;
    if !appearance.night_shift_enabled.get() {
        return false;
    }
    let start = appearance.night_shift_start.minutes_from_midnight();
    let end = appearance.night_shift_end.minutes_from_midnight();
    if start < end {
        start <= now_minutes && now_minutes < end
    } else {
        now_minutes >= start || now_minutes < end
    }
}

/// The palette the desktop was last moved onto, or `""` if it never has (garage:3714-3726).
///
/// Written by `push_theme()` and nothing else -- in particular not by [`render_theme`], which
/// writes the toolkit files -- so it is by definition the scheme currently live rather than
/// the scheme last generated. A missing or unreadable marker answers `""`, which is what the
/// Python's bare `except OSError` does, and which no [`Scheme`] can equal: a first session has
/// applied nothing, and the change gate must read that as "the apply is still owed".
#[must_use]
pub fn applied_scheme(paths: &Paths) -> String {
    std::fs::read_to_string(&paths.markers.color_scheme)
        .map_or_else(|_| String::new(), |text| text.trim().to_owned())
}

/// A palette role that must be an opaque hex, with a real error if it is not
/// (garage:3736-3749).
///
/// The Qt and hyprlock renderers can only spell `#rrggbb`; handed a composited role they
/// would write the literal text `rgba(...)` into a file whose parser takes neither, and Qt
/// would silently fall back to Fusion's own grey. Better to fail here, where the mapping
/// table is, than on the next login.
///
/// # Errors
///
/// [`RenderError::CompositedRole`] when the role resolves to anything other than
/// `is_hex_color()`'s `#` plus six hex digits -- which includes a name that is not a
/// `PALETTE` role at all, since the Python would raise `KeyError` there and this is the same
/// refusal one step earlier.
pub fn opaque(scheme: Scheme, role: &str) -> Result<&'static str, RenderError> {
    let value = palette_role(scheme, role).unwrap_or("");
    if is_hex_color(value) {
        Ok(value)
    } else {
        Err(RenderError::CompositedRole {
            role: py_str_repr(role),
            value: py_str_repr(value),
        })
    }
}

/// `is_hex_color()` (garage:1958-1961): exactly `#` and six hex digits, either case.
fn is_hex_color(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|digits| {
        digits.len() == 6 && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

/// A `PALETTE` role that is opaque in both schemes by construction, for the two call sites
/// that read one outside a toolkit writer. `on_bg` is one of `PALETTE`'s fixed roles and is
/// `#000000` / `#ffffff`; `palette::parity::every_role_is_defined_for_both_schemes` pins the
/// table against the Python source, so a miss here would be a broken build rather than a
/// condition a running renderer could reach.
/// Minutes since local midnight, which is the whole of what the two schedules compare.
///
/// `tz-rs` reads `TZ` first and `/etc/localtime` after it, exactly as libc does for the
/// Python's `time.localtime()`, so both binaries resolve the same local time on the same
/// machine. A machine with neither falls back to UTC, which is libc's own fallback: the
/// Python cannot fail here, so neither may this.
fn local_minutes_of_day() -> u16 {
    let Ok(now) = tz::UtcDateTime::now() else {
        return 0;
    };
    let here = tz::TimeZone::local()
        .ok()
        .and_then(|local| now.project(local.as_ref()).ok());
    let (hour, minute) = here.map_or((now.hour(), now.minute()), |local| {
        (local.hour(), local.minute())
    });
    u16::from(hour) * 60 + u16::from(minute)
}

#[cfg(test)]
mod tests {
    use garage_core::schema::defaults::Defaults;
    use garage_core::schema::enums::Scheme;
    use garage_core::schema::notes::Notes;
    use garage_core::schema::Preferences;

    use super::{
        is_hex_color, local_minutes_of_day, night_shift_active_at, opaque, resolve_theme_at,
    };

    fn prefs_from(departures: &str) -> Preferences {
        let table: toml::Table = departures.parse().expect("fixture toml parses");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let mut notes = Notes::new();
        let prefs = Preferences::coerce_from(&table, &defaults, &mut notes);
        assert!(
            notes.is_empty(),
            "a table row must not need coercion: {notes:?}"
        );
        prefs
    }

    /// `(theme_mode, theme_light_at, theme_dark_at, now HH:MM, expected)`, every row
    /// generated by calling the Python's own `resolve_theme()` with `time.localtime` stubbed
    /// to that minute. The wrapping window, the equal-times case and both pinned modes are
    /// covered, including the exact boundary minutes on each side.
    const SCHEDULE: &[(&str, &str, &str, u16, Scheme)] = &[
        // Pinned modes ignore the schedule entirely, whatever the clock says.
        ("light", "07:00", "19:00", 0, Scheme::Light),
        ("light", "07:00", "19:00", 720, Scheme::Light),
        ("dark", "07:00", "19:00", 720, Scheme::Dark),
        ("dark", "07:00", "19:00", 0, Scheme::Dark),
        // The ordinary window: light from theme_light_at up to but not including dark.
        ("auto", "07:00", "19:00", 0, Scheme::Dark),
        ("auto", "07:00", "19:00", 419, Scheme::Dark),
        ("auto", "07:00", "19:00", 420, Scheme::Light),
        ("auto", "07:00", "19:00", 421, Scheme::Light),
        ("auto", "07:00", "19:00", 1139, Scheme::Light),
        ("auto", "07:00", "19:00", 1140, Scheme::Dark),
        ("auto", "07:00", "19:00", 1141, Scheme::Dark),
        ("auto", "07:00", "19:00", 1439, Scheme::Dark),
        // Wrapping: light at 19:00, dark at 07:00, so the light window spans midnight.
        ("auto", "19:00", "07:00", 0, Scheme::Light),
        ("auto", "19:00", "07:00", 419, Scheme::Light),
        ("auto", "19:00", "07:00", 420, Scheme::Dark),
        ("auto", "19:00", "07:00", 1139, Scheme::Dark),
        ("auto", "19:00", "07:00", 1140, Scheme::Light),
        ("auto", "19:00", "07:00", 1439, Scheme::Light),
        // A zero-width window is not a window: dark, at every minute of the day.
        ("auto", "07:00", "07:00", 0, Scheme::Dark),
        ("auto", "07:00", "07:00", 420, Scheme::Dark),
        ("auto", "07:00", "07:00", 421, Scheme::Dark),
        ("auto", "00:00", "00:00", 0, Scheme::Dark),
        // Midnight as one end of the window rather than inside it.
        ("auto", "00:00", "12:00", 0, Scheme::Light),
        ("auto", "00:00", "12:00", 719, Scheme::Light),
        ("auto", "00:00", "12:00", 720, Scheme::Dark),
        ("auto", "12:00", "00:00", 0, Scheme::Dark),
        ("auto", "12:00", "00:00", 720, Scheme::Light),
        ("auto", "12:00", "00:00", 1439, Scheme::Light),
    ];

    #[test]
    fn the_schedule_matches_the_python_minute_for_minute() {
        for &(mode, light_at, dark_at, now, expected) in SCHEDULE {
            let prefs = prefs_from(&format!(
                "[appearance]\ntheme_mode = \"{mode}\"\ntheme_light_at = \"{light_at}\"\ntheme_dark_at = \"{dark_at}\"\n"
            ));
            assert_eq!(
                resolve_theme_at(&prefs, now),
                expected,
                "theme_mode={mode} light_at={light_at} dark_at={dark_at} now={now}"
            );
        }
    }

    /// `(enabled, start, end, now HH:MM, expected)`, generated the same way from the
    /// Python's `night_shift_active()`.
    const NIGHT_SHIFT: &[(bool, &str, &str, u16, bool)] = &[
        (false, "18:00", "06:00", 0, false),
        (false, "18:00", "06:00", 1200, false),
        // The shipped schedule, which wraps midnight.
        (true, "18:00", "06:00", 0, true),
        (true, "18:00", "06:00", 359, true),
        (true, "18:00", "06:00", 360, false),
        (true, "18:00", "06:00", 1079, false),
        (true, "18:00", "06:00", 1080, true),
        (true, "18:00", "06:00", 1439, true),
        // A daytime window, which does not wrap.
        (true, "09:00", "17:00", 539, false),
        (true, "09:00", "17:00", 540, true),
        (true, "09:00", "17:00", 1019, true),
        (true, "09:00", "17:00", 1020, false),
        // start == end takes the wrapping branch, where every minute is inside it.
        (true, "12:00", "12:00", 0, true),
        (true, "12:00", "12:00", 720, true),
        (true, "12:00", "12:00", 1439, true),
    ];

    #[test]
    fn the_night_shift_window_matches_the_python_minute_for_minute() {
        for &(enabled, start, end, now, expected) in NIGHT_SHIFT {
            let flag = if enabled { "true" } else { "false" };
            let prefs = prefs_from(&format!(
                "[appearance]\nnight_shift_enabled = {flag}\nnight_shift_start = \"{start}\"\nnight_shift_end = \"{end}\"\n"
            ));
            assert_eq!(
                night_shift_active_at(&prefs, now),
                expected,
                "enabled={enabled} start={start} end={end} now={now}"
            );
        }
    }

    #[test]
    fn an_opaque_role_comes_back_and_a_composited_one_is_refused() {
        assert_eq!(opaque(Scheme::Dark, "accent").ok(), Some("#0a84ff"));
        assert_eq!(opaque(Scheme::Light, "fg_muted").ok(), Some("#6c6c70"));
        let refused = opaque(Scheme::Dark, "bar_bg").expect_err("a composited role is refused");
        assert_eq!(
            refused.to_string(),
            "palette role 'bar_bg' is 'rgba(28, 28, 30, 0.42)', which is composited; \
             this toolkit can only be handed an opaque colour"
        );
        let missing = opaque(Scheme::Dark, "nonesuch").expect_err("an unknown role is refused");
        assert!(missing
            .to_string()
            .starts_with("palette role 'nonesuch' is ''"));
    }

    #[test]
    fn is_hex_color_accepts_only_six_digits_behind_a_hash() {
        assert!(is_hex_color("#0a84ff"));
        assert!(is_hex_color("#0A84FF"));
        assert!(!is_hex_color("0a84ff"));
        assert!(!is_hex_color("#0a84f"));
        assert!(!is_hex_color("#0a84fff"));
        assert!(!is_hex_color("rgba(0, 0, 0, 0.12)"));
        assert!(!is_hex_color(""));
    }

    #[test]
    fn the_local_clock_answers_a_minute_of_some_day() {
        assert!(local_minutes_of_day() < 24 * 60);
    }
}
