//! The fifteen string-valued preference enums, parsed once at the boundary.
//!
//! Each mirrors one of the Python source's member sets or tuples (garage's
//! `ACCENTS`, `GLASS_BLUR_LEVELS`, `SEARCH_ENGINES`, `WORKSPACE_MODES`,
//! `WALLPAPER_FITS`, `CORNER_RADII`, `BAR_BACKGROUNDS`, `TIME_FORMATS`,
//! `DATE_FORMATS`, `FIRST_WEEKDAYS`, `THEME_TOOLKITS`'s two keys, and the
//! inline `"members"` sets on `theme_mode`, `glass_mode`, the wallpaper
//! sources and `input.accel_profile`) plus what `PREFERENCE_SCHEMA` allows
//! for each key. Every string spelling below is copied verbatim from there,
//! including the kebab-case ones (`per-display`) -- that spelling is what
//! ends up in `preferences.toml`, and a Rust variant name is free to differ
//! from it because only `as_str`/`FromStr` carry the TOML text.

/// Returned by every enum's `FromStr` when a string is not one of its
/// members. Names both the offending value and the enum it failed to parse
/// as, the way a bad key in `preferences.toml` fails `validate_preferences`.
#[derive(Debug, thiserror::Error)]
#[error("{enum_name}: not a valid member: {value:?}")]
pub struct ParseEnumError {
    enum_name: &'static str,
    value: Box<str>,
}

impl ParseEnumError {
    fn new(enum_name: &'static str, value: &str) -> Self {
        Self {
            enum_name,
            value: value.into(),
        }
    }

    /// The enum type that refused to parse.
    #[must_use]
    pub const fn enum_name(&self) -> &'static str {
        self.enum_name
    }

    /// The string that matched none of the enum's members.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Generates the boilerplate every one of the fifteen enums shares: the
/// derive set, `ALL`, `as_str`, `FromStr` and `Display`. Kept as one macro
/// rather than fifteen hand-written copies so the file stays under the
/// per-file line budget with fifteen members in it.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident { $($variant:ident = $str:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// Every member, in the order the Python source declares them.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The exact spelling this member is stored and matched as.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $str),+
                }
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = ParseEnumError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($str => Ok(Self::$variant),)+
                    _ => Err(ParseEnumError::new(stringify!($name), value)),
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    // "auto" follows theme_light_at / theme_dark_at; the other two pin it.
    // (appearance.theme_mode, garage:1012-1016)
    pub enum ThemeMode {
        Light = "light",
        Dark = "dark",
        Auto = "auto",
    }
}

string_enum! {
    // The two appearances, which are also the suffixes the per-appearance
    // wallpaper keys are spelled with: wallpaper_light, wallpaper_dark_source,
    // and so on. Distinct from ThemeMode: this is the *resolved* scheme, with
    // no "auto" of its own. (SCHEMES, garage:~330)
    pub enum Scheme {
        Light = "light",
        Dark = "dark",
    }
}

string_enum! {
    // The old scale started where this one ends: its Normal and Large both
    // amplified past half the height of a ~32 px control, so the shell
    // clamped them to the same pill and the setting had no visible effect on
    // any button. Renaming the steps rather than adding a fourth keeps three,
    // with a genuinely square option. (CORNER_RADII, garage:339-347)
    pub enum CornerRadius {
        None = "none",
        Normal = "normal",
        Large = "large",
    }
}

string_enum! {
    // appearance.glass_mode's members (garage:1064-1067).
    pub enum GlassMode {
        Liquid = "liquid",
        Frosted = "frosted",
        Off = "off",
    }
}

string_enum! {
    // The slider names: passes is an integer 1..8 and downscale is a buffer
    // scale -- between them there are only about three visibly distinct
    // settings. (GLASS_BLUR_LEVELS, garage:747-753)
    pub enum GlassBlur {
        Light = "light",
        Normal = "normal",
        Deep = "deep",
    }
}

string_enum! {
    // The complete set hyprpaper 0.8.4 accepts for fit_mode. Anything else is
    // a parse error that leaves the desktop with no wallpaper at all.
    // (WALLPAPER_FITS, garage:757-759)
    pub enum WallpaperFit {
        Cover = "cover",
        Contain = "contain",
        Fit = "fit",
        Tile = "tile",
    }
}

string_enum! {
    // appearance.wallpaper_{light,dark}_source's members (garage:971, 978).
    pub enum WallpaperSource {
        Image = "image",
        Color = "color",
    }
}

string_enum! {
    // The nine accents the Appearance pane offers, in the order it draws the
    // swatches. The names are GNOME's, because push_accent() hands them
    // straight to org.gnome.desktop.interface accent-color. (ACCENTS,
    // garage:586-596)
    pub enum AccentColor {
        Blue = "blue",
        Teal = "teal",
        Green = "green",
        Yellow = "yellow",
        Orange = "orange",
        Red = "red",
        Pink = "pink",
        Purple = "purple",
        Slate = "slate",
    }
}

string_enum! {
    // SEARCH_ENGINES' keys (garage:758-764); url_template() below carries the
    // values.
    pub enum SearchEngine {
        Google = "google",
        DuckDuckGo = "duckduckgo",
        Bing = "bing",
        Kagi = "kagi",
        Custom = "custom",
    }
}

string_enum! {
    // "blurred" is the translucent tint PALETTE already carries in bar_bg;
    // "transparent" is that same colour at zero alpha, so only Hyprland's
    // blur layer is left behind the bar. (BAR_BACKGROUNDS, garage:184)
    pub enum BarBackground {
        Blurred = "blurred",
        Transparent = "transparent",
    }
}

string_enum! {
    // libinput's own range, and its own two profiles. (input.accel_profile,
    // garage:1199-1202)
    pub enum AccelProfile {
        Flat = "flat",
        Adaptive = "adaptive",
    }
}

string_enum! {
    // How a workspace relates to a display. (WORKSPACE_MODES, garage:842)
    pub enum WorkspaceMode {
        PerDisplay = "per-display",
        Shared = "shared",
    }
}

string_enum! {
    // The clock halves, as strftime fragments. Both the bar and the shell's
    // own clock are built from these, so the two cannot drift apart.
    // (TIME_FORMATS, garage:319)
    pub enum TimeFormat {
        Twelve = "12",
        TwentyFour = "24",
    }
}

string_enum! {
    // (DATE_FORMATS, garage:320)
    pub enum DateFormat {
        Dmy = "dmy",
        Mdy = "mdy",
        Iso = "iso",
    }
}

string_enum! {
    // waybar's calendar has no first-weekday option of its own, only the ISO
    // 8601 switch -- which is defined as weeks beginning on Monday.
    // (FIRST_WEEKDAYS, garage:323)
    pub enum FirstDayOfWeek {
        Sunday = "sunday",
        Monday = "monday",
    }
}

impl CornerRadius {
    // Rounding in px per corner radius setting. Both Hyprland and the shell
    // draw a visible radius of `rounding * CORNER_POWER / 2` (CORNER_POWER =
    // 3.37), so these read as roughly 0 / 12 / 24 px on screen even though
    // the stored numbers are 0 / 7 / 14. (CORNER_RADII, garage:339-347)
    #[must_use]
    pub const fn px(self) -> u16 {
        match self {
            Self::None => 0,
            Self::Normal => 7,
            Self::Large => 14,
        }
    }
}

impl GlassBlur {
    // "normal" is exactly what ran before this control existed: the plugin's
    // own defaults (4 passes at 0.25) and decorations.lua's size 6 / 2
    // passes. (GLASS_BLUR_LEVELS, garage:747-753)
    #[must_use]
    pub const fn passes(self) -> u8 {
        match self {
            Self::Light => 2,
            Self::Normal => 4,
            Self::Deep => 6,
        }
    }

    // Downscale moves against the depth on purpose: a lighter blur wants a
    // larger buffer to keep detail, a deeper one a smaller buffer that both
    // widens the kernel and pays for the extra passes.
    #[must_use]
    pub const fn downscale(self) -> f64 {
        match self {
            Self::Light => 0.40,
            Self::Normal => 0.25,
            Self::Deep => 0.18,
        }
    }

    #[must_use]
    pub const fn size(self) -> u8 {
        match self {
            Self::Light => 4,
            Self::Normal => 6,
            Self::Deep => 8,
        }
    }

    #[must_use]
    pub const fn hypr_passes(self) -> u8 {
        match self {
            Self::Light => 1,
            Self::Normal => 2,
            Self::Deep => 3,
        }
    }
}

impl AccentColor {
    // The hex each accent is drawn as. The hexes are the ones GNOME draws for
    // these names, so a GTK app and the shell agree on what "teal" is.
    // (ACCENTS, garage:586-596)
    #[must_use]
    pub const fn hex(self) -> &'static str {
        match self {
            Self::Blue => "#3584e4",
            Self::Teal => "#2190a4",
            Self::Green => "#3a944a",
            Self::Yellow => "#f6d32d",
            Self::Orange => "#ff7800",
            Self::Red => "#e01b24",
            Self::Pink => "#d56199",
            Self::Purple => "#9141ac",
            Self::Slate => "#6f8396",
        }
    }
}

impl SearchEngine {
    // URL templates; %s is replaced with the percent-encoded query.
    // (SEARCH_ENGINES, garage:758-764)
    #[must_use]
    pub const fn url_template(self) -> &'static str {
        match self {
            Self::Google => "https://www.google.com/search?q=%s",
            Self::DuckDuckGo => "https://duckduckgo.com/?q=%s",
            Self::Bing => "https://www.bing.com/search?q=%s",
            Self::Kagi => "https://kagi.com/search?q=%s",
            Self::Custom => "",
        }
    }
}

impl TimeFormat {
    #[must_use]
    pub const fn strftime(self) -> &'static str {
        match self {
            Self::Twelve => "%I:%M %p",
            Self::TwentyFour => "%H:%M",
        }
    }
}

impl DateFormat {
    #[must_use]
    pub const fn strftime(self) -> &'static str {
        match self {
            Self::Dmy => "%a %d %b",
            Self::Mdy => "%a %b %d",
            Self::Iso => "%a %Y-%m-%d",
        }
    }
}

impl FirstDayOfWeek {
    // waybar's iso8601 calendar switch: weeks begin on Monday when set.
    #[must_use]
    pub const fn iso8601(self) -> bool {
        match self {
            Self::Sunday => false,
            Self::Monday => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AccelProfile, AccentColor, BarBackground, CornerRadius, DateFormat, FirstDayOfWeek,
        GlassBlur, GlassMode, Scheme, SearchEngine, ThemeMode, TimeFormat, WallpaperFit,
        WallpaperSource, WorkspaceMode,
    };

    macro_rules! round_trip_test {
        ($fn_name:ident, $ty:ty) => {
            #[test]
            fn $fn_name() {
                for member in <$ty>::ALL {
                    let text = member.as_str();
                    let parsed: $ty = text.parse().unwrap();
                    assert_eq!(parsed, *member);
                    assert_eq!(parsed.to_string(), text);
                }
            }
        };
    }

    round_trip_test!(theme_mode_round_trips, ThemeMode);
    round_trip_test!(scheme_round_trips, Scheme);
    round_trip_test!(corner_radius_round_trips, CornerRadius);
    round_trip_test!(glass_mode_round_trips, GlassMode);
    round_trip_test!(glass_blur_round_trips, GlassBlur);
    round_trip_test!(wallpaper_fit_round_trips, WallpaperFit);
    round_trip_test!(wallpaper_source_round_trips, WallpaperSource);
    round_trip_test!(accent_color_round_trips, AccentColor);
    round_trip_test!(search_engine_round_trips, SearchEngine);
    round_trip_test!(bar_background_round_trips, BarBackground);
    round_trip_test!(accel_profile_round_trips, AccelProfile);
    round_trip_test!(workspace_mode_round_trips, WorkspaceMode);
    round_trip_test!(time_format_round_trips, TimeFormat);
    round_trip_test!(date_format_round_trips, DateFormat);
    round_trip_test!(first_day_of_week_round_trips, FirstDayOfWeek);

    #[test]
    fn rejects_bogus_value() {
        let err = "not-a-mode".parse::<ThemeMode>().unwrap_err();
        assert_eq!(err.value(), "not-a-mode");
        assert_eq!(err.enum_name(), "ThemeMode");
    }

    #[test]
    fn corner_radius_maps_to_px() {
        assert_eq!(CornerRadius::None.px(), 0);
        assert_eq!(CornerRadius::Normal.px(), 7);
        assert_eq!(CornerRadius::Large.px(), 14);
    }

    #[test]
    fn glass_blur_maps_to_settings() {
        assert_eq!(GlassBlur::Normal.passes(), 4);
        assert!((GlassBlur::Normal.downscale() - 0.25).abs() < f64::EPSILON);
        assert_eq!(GlassBlur::Normal.size(), 6);
        assert_eq!(GlassBlur::Normal.hypr_passes(), 2);
    }

    #[test]
    fn accent_color_maps_to_hex() {
        assert_eq!(AccentColor::Blue.hex(), "#3584e4");
        assert_eq!(AccentColor::Slate.hex(), "#6f8396");
    }

    #[test]
    fn time_and_date_formats_map_to_strftime() {
        assert_eq!(TimeFormat::TwentyFour.strftime(), "%H:%M");
        assert_eq!(TimeFormat::Twelve.strftime(), "%I:%M %p");
        assert_eq!(DateFormat::Iso.strftime(), "%a %Y-%m-%d");
    }

    #[test]
    fn first_day_of_week_maps_to_iso8601() {
        assert!(!FirstDayOfWeek::Sunday.iso8601());
        assert!(FirstDayOfWeek::Monday.iso8601());
    }
}
