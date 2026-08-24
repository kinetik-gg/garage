//! The preference table. One entry per key, and nothing else declares one.
//!
//! ADDING A SETTING. It used to cost seven touch points across three
//! languages: `preferences.defaults.toml`, `FALLBACK_DEFAULTS`, a branch in
//! `validate_preferences()`, a renderer, a branch in
//! `apply_changed_preference()`, plus the QML control and its metadata.
//! Nothing checked that the five Python-side ones agreed, and two of the five
//! failed *silently* when they were forgotten:
//!
//!   * a key with no branch in `validate_preferences()` was simply never
//!     checked. Not hypothetical: `night_shift_enabled`, the two wallpaper
//!     paths, `natural_scroll` and the three touchpad switches shipped that
//!     way for releases, and the table below is the first thing to make it
//!     visible -- they are the unchecked kinds now
//!     ([`PyTruthy`](crate::schema::coerce::PyTruthy) and
//!     [`UncheckedString`](crate::schema::coerce::UncheckedString)), each with
//!     the reason it is safe written next to it;
//!   * a key in a section whose apply branch dispatched by section rather than
//!     by name -- input, lock, region, workspaces -- was applied by whatever
//!     that section happened to do, which is right by luck rather than by
//!     declaration.
//!
//! The table below is now the only place. One addition is:
//!
//!   1. one entry here -- the dotted key, the kind with its constraints, the
//!      reason the constraint is what it is, and the route `set` takes to move
//!      the running desktop onto the new value;
//!   2. one line in `preferences.defaults.toml`, which stays the shipped
//!      source of truth for values. Deliberately not generated from this
//!      table: a checked-in, commented, hand-editable file is worth more than
//!      a build step, and `defaults_file_has_every_key` is the sync check -- a
//!      key in one and not the other fails the build rather than drifting;
//!   3. a renderer, but only if the setting reaches a surface no existing
//!      `render_*` function already writes. Most do not;
//!   4. the QML control.
//!
//! Derived from what is below and never hand-maintained again: the shipped
//! defaults' shape, the sections and keys `set` will accept, the whole of the
//! validation pass, and the key -> route table.
//!
//! WHAT AN ENTRY SAYS. The type is the kind: `bool`, `String` (`free_string`),
//! [`NonEmptyString`], [`Locale`], an enum from [`enums`](crate::schema::enums)
//! (`"members"` is the enum itself), [`ClockTime`], [`HexColor`],
//! [`RangedInt`] and [`RangedFloat`] (inclusive both ends), [`WorkspaceCounts`]
//! (rewritten rather than checked), and the two unchecked kinds above. `=>`
//! names the [`Route`] `set` walks after the file is written; no route means
//! the key is not settable at all. `renames` carries withdrawn spellings,
//! applied before the check and *silently*: a name this build no longer offers
//! is not a bad value, it is the same choice under the name it had when it was
//! made.
//!
//! Entry order is the order the validation pass walks, which is the order
//! coercion notes reach the journal and the order the departures are written.
//! It matches `preferences.defaults.toml` key for key
//! (`declared_order_matches_the_file`), which is what makes the emitted file
//! come out in the shipped file's own order. The one ordering the Python
//! called load-bearing -- `search_custom_url`'s cross-key check reads
//! `search_engine` -- holds here for free: that check is a second pass over
//! already-coerced fields, so neither key has to precede the other.
//!
//! A key with no entry here does not exist: `set` cannot name it, the
//! validation pass never sees it, and the departures walk drops it.

use crate::schema::coerce::{
    GlassTransparency, Locale, MotionSpeed, NonEmptyString, PaddingScale, PointerSensitivity,
    PyTruthy, RangedFloat, RangedInt, UncheckedString, UnitInterval, WorkspaceCounts,
};
use crate::schema::defaults::{DefaultsError, MissingDefault};
use crate::schema::enums::{
    AccelProfile, AccentColor, BarBackground, BarPosition, CornerRadius, DateFormat,
    FirstDayOfWeek, GlassBlur, GlassMode, SearchEngine, ThemeMode, TimeFormat, WallpaperFit,
    WallpaperSource, WorkspaceMode,
};
use crate::schema::macros::preferences;
use crate::schema::newtypes::{ClockTime, HexColor};
use crate::schema::routes::Route;

/// A dotted key that names no preference this build has.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("Unknown preference: {0}")]
pub struct ParseKeyError(String);

impl ParseKeyError {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.to_string())
    }

    /// The spelling that named nothing.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.0
    }
}

/// Why `set` refused a key outright.
///
/// Not why it refused a *value*: a value this build cannot render is coerced
/// and reported, never refused, because "a single bad value used to raise, and
/// that one raise took the whole product down with it".
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum SetError {
    /// The key carries no route, which the schema defines as not settable at
    /// all. No key in the table below is in that state today; the arm exists
    /// so adding one is a refusal rather than a silent write.
    #[error("Unsupported preference: {key}")]
    NotSettable {
        /// The key that has nowhere to go.
        key: PreferenceKey,
    },
    /// The compiled defaults -- the value a refused one is put back onto --
    /// could not be read.
    #[error(transparent)]
    Defaults(#[from] DefaultsError),
}

preferences! {
    /// The look of the desktop: its picture, its palette, its material.
    Appearance = appearance {
        /// The desktop picture belongs to the appearance, so light and dark
        /// each keep their own image, source and colour. Spelled out per
        /// appearance rather than looped over the schemes because each really
        /// is a key of its own, with its own default and its own route: only
        /// the half that is on screen may reach the desktop, or dressing the
        /// light appearance from a dark session would change what is behind
        /// the pane.
        ///
        /// The two image paths are unchecked, which is a gap the table makes
        /// visible rather than one it introduces: nothing here has ever
        /// confirmed the file exists, and `wallpaper_target()` already falls
        /// back when it does not.
        WallpaperLight = wallpaper_light: UncheckedString => WallpaperLight;
        WallpaperDark = wallpaper_dark: UncheckedString => WallpaperDark;
        WallpaperLightSource = wallpaper_light_source: WallpaperSource => WallpaperLight;
        WallpaperDarkSource = wallpaper_dark_source: WallpaperSource => WallpaperDark;
        WallpaperLightColor = wallpaper_light_color: HexColor => WallpaperLight;
        WallpaperDarkColor = wallpaper_dark_color: HexColor => WallpaperDark;
        /// Fit is shared -- it describes how an image meets the displays,
        /// which does not change with the palette -- so it always reaches the
        /// desktop. "stretch" was offered before anyone checked it against
        /// hyprpaper, which only knows the four fits. Anything else is a parse
        /// error that leaves the desktop with no wallpaper at all.
        WallpaperFit = wallpaper_fit: WallpaperFit => Wallpaper, renames ["stretch" => "cover"];
        /// The swatch name the palette is built from. Held as a name rather
        /// than a colour because the accent drives a whole generated palette,
        /// not one fill; the members come from the same table the hexes are
        /// drawn from, so an accent the pane can pick but the palette cannot
        /// draw would be impossible rather than a silent fall back to blue.
        AccentColor = accent_color: AccentColor => Accent;
        /// Unchecked, and the schedule keys beside it are not: a non-bool here
        /// is read through `bool()` by `night_shift_active()` and cannot fail,
        /// where a malformed time would reach `minutes()` and raise.
        NightShiftEnabled = night_shift_enabled: PyTruthy => NightShift;
        NightShiftStart = night_shift_start: ClockTime => NightShift;
        NightShiftEnd = night_shift_end: ClockTime => NightShift;
        /// Kelvin. The range is hyprsunset's: below 1000 the screen goes
        /// orange enough to be unusable, above 10000 it stops being a shift at
        /// all.
        NightShiftTemperature = night_shift_temperature: RangedInt<1000, 10000> => NightShift;
        /// "auto" follows `theme_light_at` / `theme_dark_at`; the other two
        /// pin it.
        ThemeMode = theme_mode: ThemeMode => Theme;
        /// The same withdrawn-name case as `wallpaper_fit`, one step further
        /// on: a file that escaped the version stamp -- restored from a
        /// backup, or hand-edited -- still carries a name from the old corner
        /// radius scale, and it means what the new name says. Only the half
        /// that cannot be confused with a current name is here, so it is safe
        /// to apply unconditionally; the ambiguous half ("normal", which used
        /// to mean what "large" means now) is gated on the stamp by the
        /// migration instead.
        CornerRadius = corner_radius: CornerRadius => CornerRadius, renames ["small" => "normal"];
        /// px per window edge. Ten is already a frame rather than a border.
        BorderSize = border_size: RangedInt<0, 10> => Border;
        ReduceMotion = reduce_motion: bool => Motion;
        /// A multiplier over the shipped animation durations; see
        /// [`MotionSpeed`] for why the ends are where they are.
        AnimationSpeed = animation_speed: RangedFloat<MotionSpeed> => Motion;
        ThemeLightAt = theme_light_at: ClockTime => Theme;
        ThemeDarkAt = theme_dark_at: ClockTime => Theme;
        SearchEngine = search_engine: SearchEngine => Search;
        /// Only a template a query cannot be put into is dropped. An empty one
        /// is the state the pane is in for as long as it takes to fill the
        /// field in after choosing "custom", and `render_search_engine()`
        /// already falls back for it -- so clearing this when the engine is
        /// not "custom" would make custom impossible to select. Hence the
        /// cross-key check in `cross_key_checks` rather than a stricter kind:
        /// the condition is about `search_engine`, not about this value alone.
        SearchCustomUrl = search_custom_url: String => Search;
        GlassMode = glass_mode: GlassMode => Glass;
        GlassBlur = glass_blur: GlassBlur => Glass;
        GlassTransparency = glass_transparency: RangedFloat<GlassTransparency> => Glass;
        /// px of refracted edge. Below 4 there is nothing to see; past 128 the
        /// edge is wider than most of the controls it is drawn on.
        GlassEdgeWidth = glass_edge_width: RangedInt<4, 128> => Glass;
        /// The three unit-interval material weights. 0 and 1 are both
        /// meaningful ends, so the range is closed on both.
        GlassRefraction = glass_refraction: RangedFloat<UnitInterval> => Glass;
        GlassClarity = glass_clarity: RangedFloat<UnitInterval> => Glass;
        GlassHighlight = glass_highlight: RangedFloat<UnitInterval> => Glass;
    }

    /// The menu bar. Two routes out, and both end at the same republished
    /// layout marker -- the split survives because it says what a key *means*:
    /// a `BarStyle` key shapes the bar's chrome, a `BarWidgets` key decides
    /// what is in it, and the shell reads the whole document either way.
    Bar = bar {
        /// px on the bar's thin axis, and the range is what a bar can be:
        /// below thirty the workspace dots and the widget icons stop fitting,
        /// above sixty it is a panel rather than a bar.
        BarHeight = height: RangedInt<30, 60> => BarWidgets;
        /// A multiplier over the shipped spacing table. One is that spacing
        /// exactly, two is as loose as the bar gets before the right side runs
        /// into the clock; the default sits just above one because the shipped
        /// values were drawn for a 36px bar and the bar is taller now.
        BarPaddingScale = padding_scale: RangedFloat<PaddingScale> => BarStyle;
        /// From the same two names the stylesheet's alpha is written from, so
        /// the offered names and what is drawn for them cannot come apart.
        BarBackground = background: BarBackground => BarStyle;
        /// Which screen edge the bar docks to. The drag gesture ends in a
        /// `set` on this same key, so a drop and a typed command cannot
        /// disagree about where the bar is.
        BarPosition = position: BarPosition => BarStyle;
        /// The three rails, one extension id per line. The newline-separated
        /// lists stay scalars because this schema's leaves deliberately
        /// exclude list values (the `indexing.directories` precedent), and the
        /// ids are deliberately unchecked: which extensions exist is the
        /// shell registry's to know, so an id it cannot find is skipped there
        /// rather than refused here -- a third-party widget needs no schema
        /// change.
        BarWidgetsLeft = widgets_left: String => BarWidgets;
        BarWidgetsCenter = widgets_center: String => BarWidgets;
        BarWidgetsRight = widgets_right: String => BarWidgets;
        /// Past this many widgets a rail folds the excess behind a chevron.
        /// Two is the least a fold can leave visible, sixteen is more than any
        /// rail can draw; the default keeps the shipped right rail from ever
        /// folding.
        BarMaxGroupWidgets = max_group_widgets: RangedInt<2, 16> => BarWidgets;
    }

    /// The two settings that are about the session rather than about a surface.
    General = general {
        /// Held as a desktop id rather than a command so the pane can show the
        /// name and icon the application ships. An id that is no longer
        /// installed is left alone -- any string will do -- because
        /// `resolve_terminal()` falls back on its own, and the choice is worth
        /// keeping in case the application comes back.
        Terminal = terminal: String => Terminal;
        BuiltinLauncher = builtin_launcher: bool => Launcher;
    }

    /// Filename-only search, maintained outside the shell by a background
    /// service. Every setting restarts that service so the newly selected
    /// roots, depth or cadence takes effect with an immediate complete
    /// refresh.
    Indexing = indexing {
        IndexingEnabled = enabled: bool => FileIndex;
        IndexingFrequencyMinutes = frequency_minutes: RangedInt<1, 1440> => FileIndex;
        IndexingMaxDepth = max_depth: RangedInt<1, 64> => FileIndex;
        /// The newline-separated roots stay a scalar because this schema's
        /// leaves deliberately exclude list values; the pane presents them as
        /// a list and never asks the user to edit the serialization.
        IndexingDirectories = directories: String => FileIndex;
    }

    /// Pointer, keyboard and touchpad, all of which reach the compositor
    /// through the same reload.
    Input = input {
        /// libinput's own range, and its own two profiles.
        PointerSensitivity = pointer_sensitivity: RangedFloat<PointerSensitivity> => Input;
        AccelProfile = accel_profile: AccelProfile => Input;
        /// The four switches here are unchecked, and were before this table:
        /// every one reaches the generated Lua through `bool()`, which cannot
        /// fail whatever the file says. Declared all the same, because a key
        /// absent from the table is a key `set` refuses.
        NaturalScroll = natural_scroll: PyTruthy => Input;
        /// An xkb layout name, not checked against the installed set: xkb
        /// resolves names the session has never seen, and an empty one would
        /// leave the compositor with no layout at all.
        KeyboardLayout = keyboard_layout: NonEmptyString => Input;
        /// Hyprland's own bounds for `repeat_rate` (Hz) and `repeat_delay`
        /// (ms). Outside them the compositor either drops the setting or
        /// repeats unusably.
        RepeatRate = repeat_rate: RangedInt<1, 100> => Input;
        RepeatDelay = repeat_delay: RangedInt<100, 2000> => Input;
        TouchpadTapToClick = touchpad_tap_to_click: PyTruthy => Input;
        TouchpadNaturalScroll = touchpad_natural_scroll: PyTruthy => Input;
        TouchpadDisableWhileTyping = touchpad_disable_while_typing: PyTruthy => Input;
    }

    /// Seconds, and 0 means never. A day is the ceiling on all three: past it
    /// the timeout is indistinguishable from off and hypridle would rather be
    /// told so.
    Lock = lock {
        LockTimeout = lock_timeout: RangedInt<0, 86400> => Idle;
        DisplayOffTimeout = display_off_timeout: RangedInt<0, 86400> => Idle;
        SuspendTimeout = suspend_timeout: RangedInt<0, 86400> => Idle;
    }

    /// How workspaces relate to displays, and how many of them there are.
    Workspaces = workspaces {
        WorkspacesMode = mode: WorkspaceMode => Workspaces;
        /// Ten is the ceiling because there are only ten number keys to bind.
        WorkspacesDefaultCount = default_count: RangedInt<1, 10> => Workspaces;
        /// Normalised rather than checked in place, and never reported: the
        /// pane sends the whole map on every edit, so rewriting it as whatever
        /// parsing understood means a hand-typed stray comma is dropped once
        /// instead of being carried back out to the file on every later save.
        /// The parse is lenient by design, so there is no invalid value here
        /// to coerce, only a value to restate.
        WorkspacesCounts = counts: WorkspaceCounts => Workspaces;
        /// The size of the shared pool, kept apart from the per-display counts
        /// because the two modes answer different questions and switching
        /// between them must not rewrite the other one's answer.
        WorkspacesSharedCount = shared_count: RangedInt<1, 10> => Workspaces;
    }

    /// Language, clock and calendar, which all reach the bar clock.
    Region = region {
        /// The set of locales is whatever `locale-gen` last produced, so a
        /// name that was valid when it was chosen can disappear from under the
        /// file. "" is the no-override state, which always works -- and the
        /// kind short-circuits on it, so the common case never shells out to
        /// `locale -a`.
        RegionLocale = locale: Locale => Locale;
        RegionTimeFormat = time_format: TimeFormat => Region;
        RegionDateFormat = date_format: DateFormat => Region;
        RegionFirstDayOfWeek = first_day_of_week: FirstDayOfWeek => Region;
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use super::{PreferenceKey, Preferences, Section, SetError};
    use crate::schema::coerce::PyTruthy;
    use crate::schema::defaults::Defaults;
    use crate::schema::enums::{SearchEngine, WallpaperFit};
    use crate::schema::notes::Notes;

    const DEFAULTS_FILE: &str =
        include_str!("../../../../../desktop/.config/garage/preferences.defaults.toml");

    fn defaults() -> Defaults {
        Defaults::compiled().unwrap()
    }

    fn coerce(text: &str) -> (Preferences, Notes) {
        let raw: toml::Table = text.parse().unwrap();
        let mut notes = Notes::new();
        let prefs = Preferences::coerce_from(&raw, &defaults(), &mut notes);
        (prefs, notes)
    }

    /// The declaration order *is* the shipped file's order, per section. The
    /// departures document is written by walking the defaults, so this is what
    /// pins the order a rewritten `preferences.toml` comes out in.
    #[test]
    fn declared_order_matches_the_file() {
        let file: toml::Table = DEFAULTS_FILE.parse().unwrap();
        for section in Section::ALL.iter().copied() {
            let stored: Vec<&str> = file
                .get(section.as_str())
                .and_then(toml::Value::as_table)
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            let declared: Vec<&str> = PreferenceKey::ALL
                .iter()
                .filter(|key| key.section() == section)
                .map(|key| key.name())
                .collect();
            assert_eq!(declared, stored, "[{section}]");
        }
    }

    /// `[schema]` is the file's ninth section and deliberately not a
    /// preference: the version stamp is bookkeeping, written unconditionally.
    #[test]
    fn the_file_carries_exactly_the_declared_sections_plus_the_stamp() {
        let file: toml::Table = DEFAULTS_FILE.parse().unwrap();
        let declared: Vec<&str> = Section::ALL.iter().map(|s| s.as_str()).collect();
        let stored: Vec<&str> = file
            .keys()
            .map(String::as_str)
            .filter(|name| *name != "schema")
            .collect();
        assert_eq!(declared, stored);
        assert!(file.contains_key("schema"));
    }

    #[test]
    fn keys_round_trip_through_their_dotted_spelling() {
        for key in PreferenceKey::ALL.iter().copied() {
            let dotted = key.to_string();
            assert_eq!(PreferenceKey::from_str(&dotted).unwrap(), key);
            assert_eq!(dotted, format!("{}.{}", key.section(), key.name()));
        }
        assert_eq!(PreferenceKey::ALL.len(), 62);
        assert!(PreferenceKey::from_str("appearance.nonesuch").is_err());
        assert!(PreferenceKey::from_str("nonesuch.accent_color").is_err());
        assert!(PreferenceKey::from_str("accent_color").is_err());
        assert_eq!(
            PreferenceKey::from_str("schema.preferences_version")
                .unwrap_err()
                .to_string(),
            "Unknown preference: schema.preferences_version"
        );
    }

    #[test]
    fn an_empty_file_is_the_defaults_and_costs_nothing() {
        let (prefs, notes) = coerce("");
        assert_eq!(&prefs, defaults().values());
        assert!(notes.is_empty());
        // Every section absent is the same as the file being absent.
        let (prefs, notes) = coerce("[appearance]\n");
        assert_eq!(&prefs, defaults().values());
        assert!(notes.is_empty());
    }

    #[test]
    fn a_stored_value_replaces_the_default_without_a_note() {
        let (prefs, notes) = coerce("[appearance]\nborder_size = 4\ntheme_mode = \"light\"\n");
        assert_eq!(prefs.appearance.border_size.get(), 4);
        assert!(notes.is_empty());
    }

    /// One bad value per kind, and the exact note each one costs. The strings
    /// are `validate_preferences()`'s f-string with both values repr'd; the
    /// trickiest were checked against python3 before being written here.
    #[test]
    fn every_kind_coerces_to_the_default_with_the_python_note() {
        let (prefs, notes) = coerce(
            "[appearance]\n\
             theme_mode = \"sepia\"\n\
             border_size = 99\n\
             animation_speed = 4.5\n\
             night_shift_start = \"7:30\"\n\
             wallpaper_light_color = \"blue\"\n\
             reduce_motion = 1\n\
             search_custom_url = 42\n\
             [input]\n\
             keyboard_layout = \"\"\n\
             [region]\n\
             locale = false\n",
        );
        assert_eq!(
            notes.as_slice(),
            [
                "appearance.wallpaper_light_color 'blue' is not valid, using '#f5f5f7'",
                "appearance.night_shift_start '7:30' is not valid, using '18:00'",
                "appearance.theme_mode 'sepia' is not valid, using 'dark'",
                "appearance.border_size 99 is not valid, using 0",
                "appearance.reduce_motion 1 is not valid, using False",
                "appearance.animation_speed 4.5 is not valid, using 1.0",
                "appearance.search_custom_url 42 is not valid, using ''",
                "input.keyboard_layout '' is not valid, using 'us'",
                "region.locale False is not valid, using ''",
            ]
        );
        assert_eq!(&prefs, defaults().values());
    }

    /// The cross-key check, which needs `search_engine` to decide.
    #[test]
    fn a_custom_search_url_with_no_placeholder_is_dropped() {
        let (prefs, notes) = coerce(
            "[appearance]\nsearch_engine = \"custom\"\nsearch_custom_url = \"example.com/q\"\n",
        );
        assert_eq!(
            notes.as_slice(),
            ["appearance.search_custom_url 'example.com/q' is not valid, using ''"]
        );
        assert_eq!(prefs.appearance.search_custom_url, "");

        // The same template under any other engine is kept: it is what the
        // field holds while "custom" is being chosen.
        let (prefs, notes) = coerce("[appearance]\nsearch_custom_url = \"example.com/q\"\n");
        assert!(notes.is_empty());
        assert_eq!(prefs.appearance.search_custom_url, "example.com/q");

        // An engine that is itself unusable is coerced first, and the URL is
        // then judged against the engine that survived.
        let (prefs, notes) = coerce(
            "[appearance]\nsearch_engine = \"yahoo\"\nsearch_custom_url = \"example.com/q\"\n",
        );
        assert_eq!(
            notes.as_slice(),
            ["appearance.search_engine 'yahoo' is not valid, using 'google'"]
        );
        assert_eq!(prefs.appearance.search_engine, SearchEngine::Google);
        assert_eq!(prefs.appearance.search_custom_url, "example.com/q");
    }

    #[test]
    fn renames_are_silent_and_only_apply_to_a_string() {
        let (prefs, notes) =
            coerce("[appearance]\nwallpaper_fit = \"stretch\"\ncorner_radius = \"small\"\n");
        assert_eq!(prefs.appearance.wallpaper_fit, WallpaperFit::Cover);
        assert!(notes.is_empty());

        // A hand-edited file can carry an array here, and the isinstance in
        // the Python is load-bearing rather than defensive: an unhashable
        // value must reach the kind check that has always been the thing to
        // refuse it.
        //
        // Verified against the Python before this was written, and it is the
        // one place the port is deliberately *not* bug-compatible: the rename
        // guard does hold the list back, but `value in entry["members"]` then
        // raises TypeError on an unhashable value rather than returning False,
        // so `garage` dies where this coerces and reports. The note below is
        // what the Python's own comment says should happen.
        let (prefs, notes) = coerce("[appearance]\nwallpaper_fit = [\"stretch\"]\n");
        assert_eq!(prefs.appearance.wallpaper_fit, WallpaperFit::Cover);
        assert_eq!(
            notes.as_slice(),
            ["appearance.wallpaper_fit ['stretch'] is not valid, using 'cover'"]
        );
    }

    #[test]
    fn a_withdrawn_key_disappears_without_a_word() {
        let (prefs, notes) = coerce(
            "[appearance]\nwallpaper = \"old.png\"\nwallpaper_source = \"image\"\n\
             highlight_color = \"#ff0000\"\n",
        );
        assert!(notes.is_empty());
        assert_eq!(&prefs, defaults().values());
    }

    /// An unchecked key cannot fail, whatever the file says.
    #[test]
    fn the_unchecked_kinds_never_report() {
        let (prefs, notes) = coerce(
            "[appearance]\nnight_shift_enabled = \"no\"\nwallpaper_light = []\n\
             [input]\nnatural_scroll = 0\n",
        );
        assert!(notes.is_empty());
        assert_eq!(prefs.appearance.night_shift_enabled, PyTruthy::from(true));
        assert_eq!(prefs.appearance.wallpaper_light.as_str(), "");
        assert_eq!(prefs.input.natural_scroll, PyTruthy::from(false));
    }

    #[test]
    fn workspace_counts_are_restated_rather_than_reported() {
        let (prefs, notes) = coerce("[workspaces]\ncounts = \"DP-1=3,,junk,DP-2=99\"\n");
        assert!(notes.is_empty());
        assert_eq!(prefs.workspaces.counts.as_str(), "DP-1=3,DP-2=10");
    }

    #[test]
    fn set_stores_a_value_and_coerces_a_bad_one() {
        let mut prefs = defaults().values().clone();
        let mut notes = Notes::new();
        prefs
            .set(
                PreferenceKey::BorderSize,
                &toml::Value::Integer(6),
                &mut notes,
            )
            .unwrap();
        assert_eq!(prefs.appearance.border_size.get(), 6);
        assert!(notes.is_empty());

        prefs
            .set(
                PreferenceKey::BorderSize,
                &toml::Value::Integer(99),
                &mut notes,
            )
            .unwrap();
        assert_eq!(prefs.appearance.border_size.get(), 0);
        assert_eq!(
            notes.as_slice(),
            ["appearance.border_size 99 is not valid, using 0"]
        );
    }

    /// `set` runs the cross-key pass too, exactly as the Python's `set` runs
    /// the whole validation pass after storing.
    #[test]
    fn set_runs_the_cross_key_pass() {
        let mut prefs = defaults().values().clone();
        let mut notes = Notes::new();
        prefs.appearance.search_custom_url = "example.com/q".to_string();
        prefs
            .set(
                PreferenceKey::SearchEngine,
                &toml::Value::String("custom".to_string()),
                &mut notes,
            )
            .unwrap();
        assert_eq!(prefs.appearance.search_engine, SearchEngine::Custom);
        assert_eq!(prefs.appearance.search_custom_url, "");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn get_and_each_key_agree_and_walk_in_order() {
        let prefs = defaults().values().clone();
        let mut seen = Vec::new();
        prefs.each_key(|key, value| {
            assert_eq!(value, prefs.get(key));
            seen.push(key);
        });
        assert_eq!(seen, PreferenceKey::ALL);
    }

    #[test]
    fn set_error_names_the_key() {
        // Every key is settable today, so the refusal is checked on its
        // message rather than by finding a key without a route.
        let error = SetError::NotSettable {
            key: PreferenceKey::WorkspacesCounts,
        };
        assert_eq!(
            error.to_string(),
            "Unsupported preference: workspaces.counts"
        );
    }
}
