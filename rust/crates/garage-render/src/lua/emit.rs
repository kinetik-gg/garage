//! The Lua table bodies shared between a generated fragment and a live `hyprctl eval`.
//!
//! Four builders -- `glass_options()`, `material_decoration()`, `border_general()` and
//! `motion_lua()` -- plus `lua_pairs()`, the `", "`-joining helper all four use to turn a
//! `role = value` map into the inside of a Lua table constructor. Each builder is called
//! from two places in the Python: once from the renderer that writes the value into
//! `hyprland.lua`'s generated fragment, and once from the applier that pushes the same value
//! into the running compositor with `hyprctl eval`. Sharing the builder is what keeps a
//! reload and a live push from ever disagreeing about what a setting means -- there is
//! exactly one place that turns `glass_blur = "heavy"` into Lua, and both the file and the
//! live session read it.
//!
//! `border_general()` differs from the Python in one respect, and it is a scope adaptation
//! rather than a behaviour change: the Python's `border_general(config)` resolves the theme
//! and looks the border pair up in `BORDER_COLORS` itself. `crate::palette` and `crate::theme`
//! are a *concurrent* Phase 3 task and are still doc-only stubs as of this file, and this
//! task does not own them -- so this version takes the resolved colours as arguments instead
//! of reaching for a table that does not exist yet. [`crate::preferences`] is the only
//! caller today and resolves the pair itself (see its module docs for the temporary stand-in
//! this forces); nothing about the emitted text changes.
//!
//! `glass_options()` is not a separate code path for "frosted": a bevel steepness of zero
//! leaves the surface flat, so the lateral shift is zero and the same shader produces plain
//! blur, while edge clarity and the rim light stay independent of the refraction vector and
//! so still read correctly on a flat surface. `material_decoration()` exists because
//! Hyprland's own blur and the window opacity are separate code paths from the Kinetik Glass
//! plugin -- disabling the plugin alone used to leave both still running, which is the
//! opposite of solid, so the core decoration options are driven from here instead.
//! `motion_lua()` switches animations off outright for Reduce Motion rather than shortening
//! them -- a shorter slide is still a slide, which is the thing the setting exists to
//! remove -- and keeps emitting per-leaf speeds even while motion is off, so switching it
//! back on restores the desktop without a second push.
//!
//! `corner_rounding()` lives here too, alongside the builders rather than with
//! [`crate::corner`]: windows, layer surfaces, hyprexpo's overview tiles and the shell's own
//! QML silhouettes are four separate code paths that all have to be handed the same rounding
//! in px, and this is the one function every one of `preferences.rs`'s and `corner.rs`'s Lua
//! emits it through.

use garage_core::schema::enums::GlassMode;
use garage_core::schema::Preferences;

use crate::lua::escape::lua_string;

/// Python's `format(value, "g")` with the default precision of 6 significant digits (every
/// `:g}` interpolation in `render_preferences()`, `glass_options()`, `material_decoration()`
/// and `motion_lua()`; e.g. garage:2110-2114).
///
/// Not [`garage_core::pyrepr::py_float_repr`]: that mirrors `repr()`'s *shortest round-trip*
/// digit string, which is a different algorithm from `"g"` -- `format(1.0, "g")` is `"1"`,
/// not `"1.0"`. Every float this module interpolates into a Lua fragment goes through `:g`
/// in the Python, never through `str()`/`repr()`, so `py_float_repr` is the wrong tool here.
/// Kept local rather than promoted to `garage_core::pyrepr` because this task does not own
/// that crate; flagged in this task's report as worth promoting once a second caller needs
/// it (bar spacing, the displays fragment and the live `hyprctl eval` push all use `:g` too).
#[must_use]
pub(crate) fn python_g_format(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let magnitude = value.abs();
    if magnitude == 0.0 {
        return format!("{sign}0");
    }
    let (digits, exponent) = six_significant_digits(magnitude);
    if (-4..PRECISION).contains(&exponent) {
        format!("{sign}{}", fixed_form_g(&digits, exponent + 1))
    } else {
        format!("{sign}{}", scientific_form_g(&digits, exponent))
    }
}

/// `"g"`'s default significant-digit count.
const PRECISION: i32 = 6;

/// The 6-significant-digit, correctly rounded decimal representation of a positive, finite,
/// non-zero `value`: its digit string (always [`PRECISION`] digits long) and the base-10
/// exponent of the leading digit. Delegates the rounding itself to Rust's own formatter,
/// which -- like `CPython`'s `dtoa` -- is correctly rounded for a requested precision, so
/// the two agree digit for digit.
fn six_significant_digits(value: f64) -> (String, i32) {
    let digits_after_point = usize::try_from(PRECISION - 1).unwrap_or(5);
    let scientific = format!("{value:.digits_after_point$e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|ch| *ch != '.').collect();
    (digits, exponent)
}

/// Places a decimal point `decpt` digits into `digits` (`decpt` is `CPython`'s: the value is
/// `0.<digits> * 10**decpt`), then strips insignificant trailing zeros -- and the point
/// itself if nothing remains after it. The opposite of `py_float_repr`'s
/// `Py_DTSF_ADD_DOT_0`: `"g"` never leaves a bare trailing point.
fn fixed_form_g(digits: &str, decpt: i32) -> String {
    let length = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    let placed = if decpt <= 0 {
        let zeros = "0".repeat(usize::try_from(-decpt).unwrap_or(0));
        format!("0.{zeros}{digits}")
    } else if decpt >= length {
        let zeros = "0".repeat(usize::try_from(decpt - length).unwrap_or(0));
        format!("{digits}{zeros}")
    } else {
        let (whole, fraction) = digits.split_at(usize::try_from(decpt).unwrap_or(0));
        format!("{whole}.{fraction}")
    };
    strip_insignificant(&placed)
}

/// Scientific form for the branch `"g"` takes outside `[-4, PRECISION)`: one leading digit,
/// a point, the rest -- stripped the same way the fixed branch is -- then a signed,
/// zero-padded two-digit exponent.
fn scientific_form_g(digits: &str, exponent: i32) -> String {
    let mantissa = if digits.len() <= 1 {
        digits.to_string()
    } else {
        let (lead, rest) = digits.split_at(1);
        strip_insignificant(&format!("{lead}.{rest}"))
    };
    let exponent_sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}e{exponent_sign}{:02}", exponent.abs())
}

fn strip_insignificant(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `lua_pairs()` (garage:2184-2185): join `role = value` entries into the inside of a Lua
/// table constructor.
#[must_use]
pub(crate) fn lua_pairs(pairs: &[(&str, String)], separator: &str) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{key} = {value}"))
        .collect::<Vec<_>>()
        .join(separator)
}

/// `glass_options()` (garage:2088-2115): the Kinetik Glass options the `glass_*` preferences
/// drive, as Lua literals, in the Python dict's own insertion order.
#[must_use]
pub(crate) fn glass_options(prefs: &Preferences) -> Vec<(&'static str, String)> {
    let appearance = &prefs.appearance;
    // Frosted is not a separate code path: a bevel steepness of zero leaves the surface
    // flat, so the lateral shift is zero and the same shader produces plain blur. Edge
    // clarity and the rim light are computed independently of the refraction vector, so
    // they still read correctly on a flat surface.
    let frosted = appearance.glass_mode == GlassMode::Frosted;
    let level = appearance.glass_blur;
    let refraction = if frosted {
        0.0
    } else {
        appearance.glass_refraction.get()
    };
    vec![
        (
            "enabled",
            (appearance.glass_mode != GlassMode::Off).to_string(),
        ),
        ("edge_width", appearance.glass_edge_width.get().to_string()),
        ("refraction", python_g_format(refraction)),
        (
            "edge_clarity",
            python_g_format(appearance.glass_clarity.get()),
        ),
        (
            "highlight_opacity",
            python_g_format(appearance.glass_highlight.get()),
        ),
        ("blur_passes", level.passes().to_string()),
        ("blur_downscale", python_g_format(level.downscale())),
    ]
}

/// `material_decoration()` (garage:2118-2135): the core decoration options the material
/// drives, as Lua literals. Hyprland's own blur and the window opacity are separate code
/// paths from the plugin, so disabling the plugin left both running -- driven from here
/// instead. Fullscreen stays opaque either way: there is nothing behind it worth showing
/// through.
#[must_use]
pub(crate) fn material_decoration(prefs: &Preferences) -> Vec<(&'static str, String)> {
    let appearance = &prefs.appearance;
    let level = appearance.glass_blur;
    let solid = appearance.glass_mode == GlassMode::Off;
    let opacity = if solid {
        1.0
    } else {
        1.0 - appearance.glass_transparency.get()
    };
    let blur = format!(
        "{{enabled = {}, size = {}, passes = {}}}",
        !solid,
        level.size(),
        level.hypr_passes()
    );
    vec![
        ("active_opacity", python_g_format(opacity)),
        ("inactive_opacity", python_g_format(opacity)),
        ("fullscreen_opacity", "1.0".to_string()),
        ("blur", blur),
    ]
}

/// `border_general()` (garage:2140-2150): the general options the border size drives, as a
/// Lua table body. The colour comes with the size rather than being a setting of its own --
/// `decorations.lua` paints both borders fully transparent, so a size on its own would
/// silently shrink the window and draw nothing.
///
/// See the module docs for why this takes the resolved colours as arguments rather than
/// resolving them itself the way the Python does.
#[must_use]
pub(crate) fn border_general(border_size: i64, active_rgba: &str, inactive_rgba: &str) -> String {
    format!(
        "border_size = {border_size}, col = {{active_border = \"{active_rgba}\", \
         inactive_border = \"{inactive_rgba}\"}}"
    )
}

/// One leaf `config/animations.lua` leaves switched on, with the duration it runs at when
/// the speed multiplier is 1.
struct AnimationLeaf {
    name: &'static str,
    speed: f64,
    bezier: &'static str,
    style: Option<&'static str>,
}

/// `ANIMATION_LEAVES` (garage:726-733): mirrored here rather than read back from the
/// compositor because the multiplier has to scale the configured baseline, not whatever the
/// last push happened to leave live. Every other leaf is deliberately off, so none of them
/// appear.
const ANIMATION_LEAVES: &[AnimationLeaf] = &[
    AnimationLeaf {
        name: "global",
        speed: 3.0,
        bezier: "workspaceEase",
        style: None,
    },
    AnimationLeaf {
        name: "layers",
        speed: 2.4,
        bezier: "workspaceEase",
        style: None,
    },
    AnimationLeaf {
        name: "workspaces",
        speed: 3.0,
        bezier: "workspaceEase",
        style: Some("slide"),
    },
];

/// `motion_lua()` (garage:2153-2182): the animation statements the motion preferences drive.
/// Reduce Motion switches animations off outright instead of shortening them -- shortening
/// is what the speed control already does -- and per-leaf speeds are still emitted while it
/// is on, so switching it back off restores the desktop without a second push.
#[must_use]
pub(crate) fn motion_lua(prefs: &Preferences) -> String {
    let appearance = &prefs.appearance;
    let enabled = !appearance.reduce_motion;
    // Hyprland's speed is a duration in deciseconds, so a faster desktop is a smaller
    // number: the multiplier divides the baseline rather than scaling it.
    let factor = appearance.animation_speed.get();
    let mut lines = vec![format!(
        "hl.config({{animations = {{enabled = {enabled}}}}})"
    )];
    for leaf in ANIMATION_LEAVES {
        lines.push(animation_statement(leaf, factor));
    }
    lines.join("\n") + "\n"
}

fn animation_statement(leaf: &AnimationLeaf, factor: f64) -> String {
    let mut fields = vec![
        format!("leaf = {}", lua_string(leaf.name)),
        "enabled = true".to_string(),
        format!("speed = {}", python_g_format(leaf.speed / factor)),
        format!("bezier = {}", lua_string(leaf.bezier)),
    ];
    if let Some(style) = leaf.style {
        fields.push(format!("style = {}", lua_string(style)));
    }
    format!("hl.animation({{ {} }})", fields.join(", "))
}

/// `corner_rounding()` (garage:2188-2199): the rounding in px every rounded surface on the
/// desktop is sized from. [`garage_core::schema::enums::CornerRadius::px`] carries the value
/// table; this is the one name every renderer's Lua reads it through.
#[must_use]
pub(crate) fn corner_rounding(prefs: &Preferences) -> u16 {
    prefs.appearance.corner_radius.px()
}

/// `CORNER_POWER` (garage:359): held constant across the three corner radius sizes, so a
/// corner keeps the same superellipse profile whatever size it is. Must match
/// `Theme.cornerPower` in the shell, which draws the same silhouette in QML. Lives here,
/// alongside [`corner_rounding`], rather than in `garage_core::schema::enums` because it is
/// not itself a value table -- there is only one number, not one per enum member.
pub(crate) const CORNER_POWER: f64 = 3.37;

#[cfg(test)]
mod tests {
    use super::{lua_pairs, python_g_format};

    #[test]
    fn integers_and_simple_decimals_drop_insignificant_zeros() {
        assert_eq!(python_g_format(1.0), "1");
        assert_eq!(python_g_format(0.25), "0.25");
        assert_eq!(python_g_format(3.37), "3.37");
        assert_eq!(python_g_format(0.2), "0.2");
        assert_eq!(python_g_format(100_000.0), "100000");
        assert_eq!(python_g_format(6.0), "6");
        assert_eq!(python_g_format(1.5), "1.5");
    }

    #[test]
    fn the_fixed_scientific_switchover_matches_pythons_g_format() {
        assert_eq!(python_g_format(0.0001), "0.0001");
        assert_eq!(python_g_format(0.000_01), "1e-05");
        assert_eq!(python_g_format(1e16), "1e+16");
        assert_eq!(python_g_format(123_456.0), "123456");
        assert_eq!(python_g_format(1_234_567.0), "1.23457e+06");
    }

    #[test]
    fn zero_and_signs_are_spelled_like_python() {
        assert_eq!(python_g_format(0.0), "0");
        assert_eq!(python_g_format(-0.0), "-0");
        assert_eq!(python_g_format(-1.5), "-1.5");
    }

    #[test]
    fn non_finite_values_match_python() {
        assert_eq!(python_g_format(f64::NAN), "nan");
        assert_eq!(python_g_format(f64::INFINITY), "inf");
        assert_eq!(python_g_format(f64::NEG_INFINITY), "-inf");
    }

    #[test]
    fn lua_pairs_joins_role_value_entries_with_the_given_separator() {
        let pairs = [("a", "1".to_string()), ("b", "true".to_string())];
        assert_eq!(lua_pairs(&pairs, ", "), "a = 1, b = true");
        assert_eq!(lua_pairs(&pairs, ",\n  "), "a = 1,\n  b = true");
        assert_eq!(lua_pairs(&[], ", "), "");
    }
}
