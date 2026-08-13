//! `PREFERENCE_KINDS`, as types: what a usable stored value is, per kind.
//!
//! The Python names a predicate per kind and looks it up by string. Here the
//! kind *is* the field's type, so the lookup is gone: `RangedInt<1000, 10000>`
//! carries the range `"minimum": 1000, "maximum": 10000` used to, an enum
//! carries its own member list, and a key whose type cannot hold a bad value
//! cannot be validated wrong.
//!
//! Two traits, because they are two directions. [`Coerce`] is the file coming
//! in -- `Option<Self>`, where `None` is exactly the Python predicate returning
//! `False` and costs the caller a note. [`Store`] is the value going back out,
//! to the departures document and to the notes.

use crate::schema::enums::{
    AccelProfile, AccentColor, BarBackground, CornerRadius, DateFormat, FirstDayOfWeek, GlassBlur,
    GlassMode, SearchEngine, ThemeMode, TimeFormat, WallpaperFit, WallpaperSource, WorkspaceMode,
};
use crate::schema::newtypes::{ClockTime, HexColor};
use crate::schema::notes::{py_str_toml, Notes};
use crate::schema::prefs::{PreferenceKey, Preferences};

/// Whether a stored value is one this build can render, and what it becomes.
///
/// `None` is the Python's predicate returning `False`: the value is not
/// refused, it is replaced by the shipped default and reported. See
/// [`field`], which is where that happens.
pub trait Coerce: Sized {
    /// The value as the stored file carries it, or `None` for one this build
    /// cannot use.
    fn coerce(value: &toml::Value) -> Option<Self>;
}

/// The other direction: the value as `preferences.toml` carries it.
///
/// Deliberately not `Into<toml::Value>`: the schema's types are not TOML
/// values with extra rules, they are product values that happen to be stored,
/// and a blanket conversion would let any of them be written anywhere.
pub trait Store {
    /// This value, as the departures document and the notes spell it.
    fn store(&self) -> toml::Value;
}

// ---------------------------------------------------------------------------
// The kinds that are not numbers

impl Coerce for bool {
    // "bool": isinstance(value, bool). TOML has a real boolean, so this is the
    // whole of it -- 1 and "true" are not bools here any more than there.
    fn coerce(value: &toml::Value) -> Option<Self> {
        value.as_bool()
    }
}

impl Store for bool {
    fn store(&self) -> toml::Value {
        toml::Value::Boolean(*self)
    }
}

impl Coerce for String {
    // "free_string": isinstance(value, str), and nothing else. The empty
    // string is a value in its own right for every key that uses this kind.
    fn coerce(value: &toml::Value) -> Option<Self> {
        value.as_str().map(ToString::to_string)
    }
}

impl Store for String {
    fn store(&self) -> toml::Value {
        toml::Value::String(self.clone())
    }
}

/// `"unchecked"`, for the five switches that reach a renderer through `bool()`.
///
/// The schema banner names seven keys that shipped with no validation at all;
/// five of them -- `night_shift_enabled`, `natural_scroll` and the three
/// touchpad switches -- are safe because every renderer reads them through
/// Python's `bool()`, which cannot fail whatever the file says. That is what
/// this type is: `bool()` itself, so the same file produces the same switch.
///
/// (The other two are the wallpaper paths, which are read as *paths* rather
/// than as truth values -- see [`UncheckedString`].)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PyTruthy(bool);

impl PyTruthy {
    /// The switch position.
    #[must_use]
    pub const fn get(self) -> bool {
        self.0
    }
}

impl From<bool> for PyTruthy {
    fn from(flag: bool) -> Self {
        Self(flag)
    }
}

impl Coerce for PyTruthy {
    // Python's truthiness, exactly: zero is false and so is every empty
    // container, a NaN is true (it is not zero), and a datetime -- an object
    // with no __bool__ and no __len__ -- is true.
    fn coerce(value: &toml::Value) -> Option<Self> {
        Some(Self(match value {
            toml::Value::Boolean(flag) => *flag,
            toml::Value::Integer(number) => *number != 0,
            toml::Value::Float(number) => *number != 0.0,
            toml::Value::String(text) => !text.is_empty(),
            toml::Value::Array(items) => !items.is_empty(),
            toml::Value::Table(entries) => !entries.is_empty(),
            toml::Value::Datetime(_) => true,
        }))
    }
}

impl Store for PyTruthy {
    fn store(&self) -> toml::Value {
        toml::Value::Boolean(self.0)
    }
}

/// `"unchecked"`, for the two wallpaper paths.
///
/// Nothing in the Python has ever confirmed the file exists, and
/// `wallpaper_target()` falls back when it does not -- it reads the key as
/// `str(value or "")`, which is this type: a stored string is itself, and
/// anything else is what Python would have stringified it to, with a falsy
/// value flattened to the empty string that means "no picture chosen".
///
/// The one visible difference is what a hand edit leaves *in the file*: the
/// Python stores whatever it was handed, so a `wallpaper_light = []` survives
/// in `preferences.toml` and is read as "" on every render, where this stores
/// the "" it renders as. The desktop is the same either way; the file is
/// corrected once, which is what the coercion pass does for every other kind.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct UncheckedString(String);

impl UncheckedString {
    /// The path as `wallpaper_target()` would use it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for UncheckedString {
    fn from(text: &str) -> Self {
        Self(text.to_string())
    }
}

impl Coerce for UncheckedString {
    fn coerce(value: &toml::Value) -> Option<Self> {
        let truthy = PyTruthy::coerce(value).is_some_and(PyTruthy::get);
        Some(Self(if truthy {
            py_str_toml(value)
        } else {
            String::new()
        }))
    }
}

impl Store for UncheckedString {
    fn store(&self) -> toml::Value {
        toml::Value::String(self.0.clone())
    }
}

/// `"nonempty_string"`: a string, and not the empty one.
///
/// One key uses it -- `input.keyboard_layout` -- because an empty layout would
/// leave the compositor with no layout at all.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct NonEmptyString(String);

impl NonEmptyString {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Coerce for NonEmptyString {
    fn coerce(value: &toml::Value) -> Option<Self> {
        value
            .as_str()
            .filter(|text| !text.is_empty())
            .map(|text| Self(text.to_string()))
    }
}

impl Store for NonEmptyString {
    fn store(&self) -> toml::Value {
        toml::Value::String(self.0.clone())
    }
}

/// `"locale"`: a string, and either empty or one `locale -a` reports.
///
/// The empty short-circuit is ported in full: `""` is the no-override state,
/// which always works, and testing it first is what keeps the common case from
/// shelling out at all.
///
/// **Parity gap, stated plainly:** the non-empty half is not checked here.
/// `installed_locales()` runs `locale -a`, and this crate may not spawn a
/// process -- see `crate::traits`, where the same rule put `luac` behind a
/// trait. A locale that was valid when it was chosen and has since been
/// removed by `locale-gen` therefore survives this pass in the Rust build and
/// is coerced away in the Python one; wiring the installed set in belongs with
/// the process-owning crate.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Locale(String);

impl Locale {
    /// The stored name, empty for "no override".
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this locale is one the machine can actually produce, given the
    /// set a caller that may run `locale -a` collected. The empty override
    /// short-circuits, exactly as the kind does.
    #[must_use]
    pub fn is_usable(&self, installed: &[String]) -> bool {
        self.0.is_empty() || installed.iter().any(|name| name == &self.0)
    }
}

impl Coerce for Locale {
    fn coerce(value: &toml::Value) -> Option<Self> {
        value.as_str().map(|text| Self(text.to_string()))
    }
}

impl Store for Locale {
    fn store(&self) -> toml::Value {
        toml::Value::String(self.0.clone())
    }
}

/// Generates the two impls every string-valued enum shares: parse the stored
/// spelling, store the spelling back. `"enum"`'s `members` is the enum itself.
macro_rules! enum_kind {
    ($($enum_name:ident),+ $(,)?) => {$(
        impl Coerce for $enum_name {
            fn coerce(value: &toml::Value) -> Option<Self> {
                value.as_str().and_then(|text| text.parse().ok())
            }
        }

        impl Store for $enum_name {
            fn store(&self) -> toml::Value {
                toml::Value::String(self.as_str().to_string())
            }
        }
    )+};
}

enum_kind!(
    AccelProfile,
    AccentColor,
    BarBackground,
    CornerRadius,
    DateFormat,
    FirstDayOfWeek,
    GlassBlur,
    GlassMode,
    SearchEngine,
    ThemeMode,
    TimeFormat,
    WallpaperFit,
    WallpaperSource,
    WorkspaceMode,
);

impl Coerce for ClockTime {
    // "time": is_clock_time(), which is the HH:MM regex and nothing else.
    fn coerce(value: &toml::Value) -> Option<Self> {
        value.as_str().and_then(|text| text.parse().ok())
    }
}

impl Store for ClockTime {
    fn store(&self) -> toml::Value {
        toml::Value::String(self.to_string())
    }
}

impl Coerce for HexColor {
    // "hex_color": is_hex_color(), `#rrggbb` in either case.
    fn coerce(value: &toml::Value) -> Option<Self> {
        value.as_str().and_then(|text| text.parse().ok())
    }
}

impl Store for HexColor {
    fn store(&self) -> toml::Value {
        // The raw spelling it was parsed from: writers pass it through
        // verbatim, so normalizing the case here would rewrite the file.
        toml::Value::String(self.as_str().to_string())
    }
}

// ---------------------------------------------------------------------------
// The kinds that live next door

// The two number kinds and the one normalised kind are in `ranged` and
// `normalise`: each carries a mechanism of its own -- a range, and a rewrite
// that happens before the check -- and both are re-exported here so the table
// names every kind through one module.
pub use crate::schema::normalise::{
    format_workspace_counts, parse_workspace_counts, WorkspaceCounts, WORKSPACE_COUNT_MAX,
};
pub use crate::schema::ranged::{
    in_range, FloatRange, GlassTransparency, MotionSpeed, Number, PaddingScale, PointerSensitivity,
    RangedFloat, RangedInt, UnitInterval,
};

/// One key of the stored file, coerced onto the shipped default.
///
/// The order is the Python's, and each step is load-bearing:
///
///   1. a key the stored file does not carry takes the default with no note --
///      the merged view the Python validates always has one;
///   2. a withdrawn *spelling* is carried across silently, because a name this
///      build no longer offers is not a bad value, it is the same choice under
///      the name it had when it was made;
///   3. the kind decides, and a value it refuses costs one note and the
///      shipped default.
#[must_use]
pub fn field<T: Coerce + Store + Clone>(
    stored: Option<&toml::Value>,
    renames: &[(&str, &str)],
    key: PreferenceKey,
    default: &T,
    notes: &mut Notes,
) -> T {
    let Some(value) = stored else {
        return default.clone();
    };
    let carried = rename(value, renames);
    let value = carried.as_ref().unwrap_or(value);
    if let Some(coerced) = T::coerce(value) {
        return coerced;
    }
    notes.push_coercion(key, value, &default.store());
    default.clone()
}

/// A withdrawn spelling mapped to what it means now, or `None` for a value
/// that is not one.
///
/// Only a string can be a spelling, and the test for one is load-bearing
/// rather than defensive: a hand-edited file can carry a TOML array here, and
/// an unhashable value must reach the kind check that has always been the
/// thing to refuse it.
fn rename(value: &toml::Value, renames: &[(&str, &str)]) -> Option<toml::Value> {
    let text = value.as_str()?;
    renames
        .iter()
        .find(|(from, _)| *from == text)
        .map(|(_, to)| toml::Value::String((*to).to_string()))
}

/// The `also` pass: the conditions that need a second key to decide.
///
/// A second pass rather than a step inside the field walk, which settles the
/// one ordering the Python's table has to be careful about -- "`search_custom_url`'s
/// cross-key check reads `search_engine`, so `search_engine` has to have been
/// coerced first". Here every field has been coerced before this runs, so the
/// declaration order of the two keys cannot matter at all.
pub fn cross_key_checks(prefs: &mut Preferences, defaults: &Preferences, notes: &mut Notes) {
    // Only a template a query cannot be put into is dropped. An empty one is
    // the state the pane is in for as long as it takes to fill the field in
    // after choosing "custom", and render_search_engine() already falls back
    // for it -- so clearing this when the engine is not "custom" would make
    // custom impossible to select.
    let url = prefs.appearance.search_custom_url.clone();
    let usable = url.is_empty()
        || url.contains("%s")
        || prefs.appearance.search_engine != SearchEngine::Custom;
    if !usable {
        let default = defaults.appearance.search_custom_url.clone();
        notes.push_coercion(
            PreferenceKey::SearchCustomUrl,
            &url.store(),
            &default.store(),
        );
        prefs.appearance.search_custom_url = default;
    }
}

#[cfg(test)]
mod tests {
    use super::{Coerce, Locale, NonEmptyString, PyTruthy, Store, UncheckedString};
    use crate::schema::enums::WallpaperFit;

    fn parse(text: &str) -> toml::Value {
        let table: toml::Table = format!("value = {text}").parse().unwrap();
        table.get("value").unwrap().clone()
    }

    #[test]
    fn bools_are_bools_and_nothing_else() {
        assert_eq!(bool::coerce(&parse("true")), Some(true));
        assert_eq!(bool::coerce(&parse("1")), None);
        assert_eq!(bool::coerce(&parse("\"true\"")), None);
    }

    #[test]
    fn py_truthy_mirrors_python_bool() {
        let truthy = |text: &str| PyTruthy::coerce(&parse(text)).map(PyTruthy::get);
        assert_eq!(truthy("true"), Some(true));
        assert_eq!(truthy("false"), Some(false));
        assert_eq!(truthy("0"), Some(false));
        assert_eq!(truthy("-1"), Some(true));
        assert_eq!(truthy("0.0"), Some(false));
        assert_eq!(truthy("nan"), Some(true));
        assert_eq!(truthy("\"\""), Some(false));
        assert_eq!(truthy("\"off\""), Some(true));
        assert_eq!(truthy("[]"), Some(false));
        assert_eq!(truthy("[0]"), Some(true));
        assert_eq!(truthy("{}"), Some(false));
        assert_eq!(truthy("1979-05-27"), Some(true));
    }

    #[test]
    fn unchecked_strings_follow_wallpaper_target() {
        let coerced = |text: &str| UncheckedString::coerce(&parse(text)).unwrap();
        assert_eq!(coerced("\"~/pic.png\"").as_str(), "~/pic.png");
        assert_eq!(coerced("\"\"").as_str(), "");
        assert_eq!(coerced("false").as_str(), "");
        assert_eq!(coerced("0").as_str(), "");
        assert_eq!(coerced("true").as_str(), "True");
    }

    #[test]
    fn nonempty_and_locale_kinds_differ_only_on_the_empty_string() {
        assert!(NonEmptyString::coerce(&parse("\"us\"")).is_some());
        assert!(NonEmptyString::coerce(&parse("\"\"")).is_none());
        assert!(NonEmptyString::coerce(&parse("7")).is_none());
        assert!(Locale::coerce(&parse("\"\"")).is_some());
        assert!(Locale::coerce(&parse("7")).is_none());
    }

    #[test]
    fn locale_checks_the_installed_set_only_when_it_is_given_one() {
        let installed = ["en_US.UTF-8".to_string()];
        assert!(Locale::coerce(&parse("\"\""))
            .unwrap()
            .is_usable(&installed));
        assert!(Locale::coerce(&parse("\"en_US.UTF-8\""))
            .unwrap()
            .is_usable(&installed));
        assert!(!Locale::coerce(&parse("\"xx_XX.UTF-8\""))
            .unwrap()
            .is_usable(&installed));
    }

    #[test]
    fn enums_parse_their_own_members() {
        assert_eq!(
            WallpaperFit::coerce(&parse("\"tile\"")),
            Some(WallpaperFit::Tile)
        );
        assert_eq!(WallpaperFit::coerce(&parse("\"stretch\"")), None);
        assert_eq!(WallpaperFit::Tile.store(), parse("\"tile\""));
    }
}
