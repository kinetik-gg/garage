//! `waybar_spacing_css()`: the padding table, scaled by `bar.padding_scale`, as the CSS rules
//! it overrides.
//!
//! Appended after the base sheet's own `@import`, which names none of these paddings any
//! more: GTK CSS has no arithmetic, so the only way a scale slider can reach a padding is for
//! the padding to be generated. Every selector here is the same selector the base sheet uses
//! for the rest of that rule, and a later rule of equal specificity wins in GTK CSS -- so the
//! fonts and colours come from the import and the spacing from here.
//!
//! One value is deliberately not scaled: the workspace dot's vertical margin. It is what
//! gives the dot's box its height, so it is derived from the bar height instead -- at scale
//! 2.0 a scaled 15px would ask for a box taller than a typical bar, and the bar would either
//! grow to fit it or clip the dots. The hover-tint margins on every other module follow the
//! same rule, for the same reason: the bar height decides them, not the density slider.
//!
//! Reduce Motion reaches this file too, because waybar cannot see Hyprland's own animation
//! switch: the workspace dot's width transition and the hover tint's fade both drop to
//! `none` here when motion is reduced, which is the bar's half of `Route::Motion`'s four
//! steps.
//!
//! Doc-only: returns a `String` composed into `waybar_style_css()` in
//! [`crate::palette::waybar`], not a write outcome of its own.

use garage_core::schema::Preferences;

use crate::lua::emit::python_g_format;

/// `PADDING_TABLE` (garage:198-217): the bar's spacing, as base pixel values at
/// `bar.padding_scale = 1.0`. Order matches the Python dict, though nothing here depends on
/// it -- each name is read by [`step`] on its own.
const PADDING_TABLE: &[(&str, f64)] = &[
    ("edge", 18.0),
    ("menu_right", 13.0),
    ("workspace_gap", 5.0),
    ("module", 12.0),
    ("image", 8.0),
    ("icon", 10.0),
    ("tray", 8.0),
    ("tooltip", 8.0),
];

/// `WORKSPACE_DOT` (garage:223): the idle workspace dot's diameter.
const WORKSPACE_DOT: i64 = 6;
/// `MODULE_HOVER_PILL` (garage:231): the height of the tint a module wears under the pointer.
const MODULE_HOVER_PILL: i64 = 27;
/// `MENU_BUTTON_NUDGE` (garage:256): optical correction for the menu glyph, taken out of the
/// square rather than added to it.
const MENU_BUTTON_NUDGE: i64 = 6;
/// `WAYBAR_STATUS_TAIL_WIDTH` (garage:172): how far the right side's static tail reaches in
/// from the edge of the output. Not scaled -- `garage-panel-toggle` reads it back out of the
/// generated stylesheet as a fixed anchor.
const WAYBAR_STATUS_TAIL_WIDTH: i64 = 320;

/// `WAYBAR_HOVER_MODULES` (garage:258-262): every module that answers the pointer, and so
/// wears [`MODULE_HOVER_PILL`]'s tint. The workspace dots are deliberately absent -- see the
/// Python's own comment, reproduced in this module's docs.
const WAYBAR_HOVER_MODULES: &[&str] = &[
    "#custom-menu",
    "#image",
    "#custom-media",
    "#custom-ai-usage",
    "#custom-notifications",
    "#custom-launcher",
    "#custom-control-center",
    "#custom-containers",
    "#custom-microphone",
    "#custom-smb",
    "#custom-git",
    "#custom-monitor",
    "#pulseaudio",
    "#clock",
];

/// `{name: max(0, round(base * scale)) for name, base in PADDING_TABLE.items()}`, one name at
/// a time rather than a whole map: nothing here reads more than one entry per call site, and
/// a lookup keeps `PADDING_TABLE` the single source the way the Python dict comprehension is.
///
/// `round()` here is Python's: round-half-to-even on the real value of `base * scale`, not
/// Rust's `f64::round()`, which rounds every half away from zero. `bar.padding_scale` values
/// like `1.25` land several of the shipped bases on an exact `.5` -- `18 * 1.25 == 22.5` --
/// where the two rules disagree, so the distinction is not academic.
///
/// The final cast is exact for every value this ever produces: a bar padding is at most a
/// couple of hundred pixels, nowhere near where `f64`-to-`i64` truncation could bite.
///
/// # Panics
///
/// If `name` is not one of [`PADDING_TABLE`]'s own keys -- unreachable from this module's
/// single call site, [`waybar_spacing_css`], which only ever names one of its own constants.
#[allow(clippy::cast_possible_truncation)]
fn step(scale: f64, name: &str) -> i64 {
    let base = PADDING_TABLE
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map_or(0.0, |(_, base)| *base);
    0.max(round_half_to_even(base * scale) as i64)
}

/// `round()` (no `ndigits`): nearest integer, ties to even. Exact for the magnitudes this
/// module deals in -- small bases times a scale with few decimal digits -- where `floor()`
/// and the fractional remainder are both computed without rounding error, which is what makes
/// the equality tests below safe rather than the usual reason the lint exists.
#[allow(clippy::float_cmp)]
fn round_half_to_even(value: f64) -> f64 {
    let floor = value.floor();
    let fraction = value - floor;
    if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else if floor.rem_euclid(2.0) == 0.0 {
        floor
    } else {
        floor + 1.0
    }
}

/// Floor division, `//`: `div_euclid` agrees with it for every divisor this module uses
/// (all positive), unlike Rust's `/`, which truncates toward zero.
fn floor_div(numerator: i64, denominator: i64) -> i64 {
    numerator.div_euclid(denominator)
}

/// Every scaled or derived number [`waybar_spacing_css`]'s template reads, computed once and
/// handed to the block builders below -- keeping each of them to one argument is what keeps
/// them under the workspace's function-length lint without a second copy of this arithmetic.
struct Metrics {
    scale_g: String,
    edge: i64,
    menu_right: i64,
    workspace_gap: i64,
    module: i64,
    image: i64,
    icon: i64,
    tray: i64,
    tooltip: i64,
    dot_margin: i64,
    workspace_button_margin: i64,
    hover_margin: i64,
    workspace_transition: &'static str,
    hover_transition: &'static str,
}

fn compute_metrics(prefs: &Preferences) -> Metrics {
    let scale = prefs.bar.padding_scale.get();
    // Centred by hand rather than left to GTK, because the margin *is* the box: the label
    // is WORKSPACE_DOT tall and this margin is the rest of the box.
    let height = prefs.bar.height.get();
    let dot_margin = 0.max(floor_div(MODULE_HOVER_PILL - WORKSPACE_DOT, 2));
    // Same arithmetic, same reason: the pill is MODULE_HOVER_PILL tall and this margin is
    // the rest of the bar. Vertical only -- a horizontal margin here would move the metric
    // strips, and the monitor dashboard is positioned from where they sit.
    let reduce_motion = prefs.appearance.reduce_motion;
    // The tint fades in unless the user has asked for no motion, in which case it appears
    // on the frame the pointer arrives. Waybar cannot see Hyprland's animation switch, so
    // Reduce Motion has to reach it here.
    Metrics {
        scale_g: python_g_format(scale),
        edge: step(scale, "edge"),
        menu_right: step(scale, "menu_right"),
        workspace_gap: step(scale, "workspace_gap"),
        module: step(scale, "module"),
        image: step(scale, "image"),
        icon: step(scale, "icon"),
        tray: step(scale, "tray"),
        tooltip: step(scale, "tooltip"),
        dot_margin,
        workspace_button_margin: 0.max(floor_div(height - (WORKSPACE_DOT + dot_margin * 2), 2)),
        hover_margin: 0.max(floor_div(height - MODULE_HOVER_PILL, 2)),
        workspace_transition: if reduce_motion {
            "none"
        } else {
            "min-width 180ms ease-out, background-color 180ms ease-out"
        },
        hover_transition: if reduce_motion {
            "none"
        } else {
            "background-color 130ms ease-out"
        },
    }
}

/// The menu square: header comment plus `#custom-menu`.
fn menu_block(m: &Metrics) -> String {
    format!(
        "/* Spacing from PADDING_TABLE at bar.padding_scale {scale_g}. */\n\
         /* Square, at the width the hover tint is tall. Its spacings are the margins\n\
         \x20  either side rather than padding, which would go inside the square and stretch\n\
         \x20  it. See MENU_BUTTON_NUDGE for the right-hand padding, which is optical\n\
         \x20  correction for the glyph and is taken out of the square rather than added. */\n\
         #custom-menu {{\n\
         \x20 min-width: {menu_width}px;\n\
         \x20 padding-left: 0;\n\
         \x20 padding-right: {MENU_BUTTON_NUDGE}px;\n\
         \x20 margin-left: {edge}px;\n\
         \x20 margin-right: {menu_right}px;\n\
         }}\n\n",
        scale_g = m.scale_g,
        menu_width = MODULE_HOVER_PILL - MENU_BUTTON_NUDGE,
        edge = m.edge,
        menu_right = m.menu_right,
    )
}

/// The workspace cluster: the button's click target, then the dot's own unscaled margin.
fn workspaces_block(m: &Metrics) -> String {
    format!(
        "/* Padding, so the space between two dots is part of a button and answers a\n\
         \x20  click. As a margin it was outside the button, which left the target the width\n\
         \x20  of the dot itself -- smaller than the pointer that has to find it. */\n\
         #workspaces button {{\n\
         \x20 margin: {workspace_button_margin}px 0;\n\
         \x20 padding: 0 {workspace_gap}px;\n\
         }}\n\
         \n\
         /* Not scaled: this margin *is* the dot's box, and the box is what the hover\n\
         \x20  tint is drawn around, so it is derived from MODULE_HOVER_PILL and the bar\n\
         \x20  height rather than from the density slider. See waybar_spacing_css(). */\n\
         #workspaces button label {{\n\
         \x20 margin: {dot_margin}px 0;\n\
         \x20 transition: {workspace_transition};\n\
         }}\n\n",
        workspace_button_margin = m.workspace_button_margin,
        workspace_gap = m.workspace_gap,
        dot_margin = m.dot_margin,
        workspace_transition = m.workspace_transition,
    )
}

/// The media control's own margin, the text modules' shared padding, and the metric strips'.
fn media_and_module_block(m: &Metrics) -> String {
    format!(
        "/* Mirror the menu-to-workspaces gap on the far side of the workspace cluster.\n\
         \x20  Outside the media control so its hover tint does not paint across the gap. */\n\
         #custom-media {{\n\
         \x20 margin-left: {menu_right}px;\n\
         }}\n\
         \n\
         #custom-media,\n\
         #custom-containers,\n\
         #custom-microphone,\n\
         #custom-smb,\n\
         #custom-git,\n\
         #custom-monitor,\n\
         #custom-ai-usage,\n\
         #pulseaudio,\n\
         #clock {{\n\
         \x20 padding: 0 {module}px;\n\
         }}\n\
         \n\
         #image {{\n\
         \x20 padding: 0 {image}px;\n\
         }}\n\n",
        menu_right = m.menu_right,
        module = m.module,
        image = m.image,
    )
}

/// The status-tail anchor and the shared hover-tint room every pointer-answering module wears.
fn status_and_hover_block(m: &Metrics) -> String {
    let hover_modules = WAYBAR_HOVER_MODULES.join(",\n");
    format!(
        "/* The one number garage-panel-toggle reads back out of this sheet, so the\n\
         \x20  dashboard's anchor and the bar's own layout cannot disagree about it. See\n\
         \x20  WAYBAR_STATUS_TAIL_WIDTH. */\n\
         #status-tail {{\n\
         \x20 min-width: {WAYBAR_STATUS_TAIL_WIDTH}px;\n\
         }}\n\
         \n\
         /* Room for the hover tint to sit in. Carried by the modules themselves and not\n\
         \x20  by their hover state, so the bar does not reflow under the pointer and only\n\
         \x20  the colour changes. Not scaled, because the bar height decides this one the\n\
         \x20  same way it decides the dot margin above. See waybar_spacing_css(). */\n\
         {hover_modules} {{\n\
         \x20 margin-top: {hover_margin}px;\n\
         \x20 margin-bottom: {hover_margin}px;\n\
         \x20 transition: {hover_transition};\n\
         }}\n\n",
        hover_margin = m.hover_margin,
        hover_transition = m.hover_transition,
    )
}

/// The icon trio, the tray, the clock's right-hand gutter and the tooltip's own padding.
fn tail_block(m: &Metrics) -> String {
    format!(
        "#custom-notifications,\n\
         #custom-launcher,\n\
         #custom-control-center {{\n\
         \x20 padding-left: {icon}px;\n\
         \x20 padding-right: {icon}px;\n\
         }}\n\
         \n\
         #tray {{\n\
         \x20 padding: 0 {tray}px;\n\
         }}\n\
         \n\
         #clock {{\n\
         \x20 margin-right: {edge}px;\n\
         }}\n\
         \n\
         tooltip {{\n\
         \x20 padding: {tooltip}px;\n\
         }}\n",
        icon = m.icon,
        tray = m.tray,
        edge = m.edge,
        tooltip = m.tooltip,
    )
}

/// `PADDING_TABLE`, scaled by `bar.padding_scale`, as the rules it overrides
/// (`waybar_spacing_css()`, garage:3854-3979).
#[must_use]
pub(crate) fn waybar_spacing_css(prefs: &Preferences) -> String {
    let m = compute_metrics(prefs);
    let mut out = menu_block(&m);
    out.push_str(&workspaces_block(&m));
    out.push_str(&media_and_module_block(&m));
    out.push_str(&status_and_hover_block(&m));
    out.push_str(&tail_block(&m));
    out
}

#[cfg(test)]
mod tests {
    use garage_core::schema::defaults::Defaults;
    use garage_core::schema::notes::Notes;
    use garage_core::schema::Preferences;

    use super::{round_half_to_even, waybar_spacing_css};

    fn prefs_from(departures: &str) -> Preferences {
        let table: toml::Table = departures.parse().expect("fixture toml parses");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let mut notes = Notes::new();
        Preferences::coerce_from(&table, &defaults, &mut notes)
    }

    /// `desktop/.local/bin/garage`'s `waybar_spacing_css(FALLBACK_DEFAULTS)`, captured with
    /// `tests/harness.py`. `bar.padding_scale` defaults to `1.2`, which lands several of the
    /// shipped bases on a non-integer product (`13 * 1.2 == 15.6`), so this alone already
    /// exercises rounding.
    const DEFAULTS_FIXTURE: &str = include_str!("../../testdata/bar_spacing_defaults.css");

    #[test]
    fn matches_the_python_backend_byte_for_byte_on_the_shipped_defaults() {
        let prefs = prefs_from("");
        assert_eq!(waybar_spacing_css(&prefs), DEFAULTS_FIXTURE);
    }

    /// A second scenario -- Reduce Motion on, `padding_scale` at `1.5` (which lands
    /// `workspace_gap` and `menu_right` on an exact `.5`, a case `1.2` does not reach), a
    /// shorter bar -- captured the same way, so the transitions dropping to `none` and a
    /// different rounding path are both pinned against the real backend rather than only
    /// against each other. `1.5` and `35` are both in `bar.padding_scale`/`bar.height`'s own
    /// validated ranges, unlike an arbitrary pair, since this test builds a real
    /// [`Preferences`] through [`Preferences::coerce_from`] rather than an unchecked dict.
    const VARIANT_FIXTURE: &str = include_str!("../../testdata/bar_spacing_variant.css");

    #[test]
    fn matches_the_python_backend_with_reduce_motion_and_a_half_integer_scale() {
        let prefs = prefs_from(
            "[appearance]\nreduce_motion = true\n[bar]\npadding_scale = 1.5\nheight = 35\n",
        );
        assert_eq!(waybar_spacing_css(&prefs), VARIANT_FIXTURE);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn round_half_to_even_matches_pythons_round_at_the_exact_halves_this_module_can_produce() {
        assert_eq!(round_half_to_even(22.5), 22.0);
        assert_eq!(round_half_to_even(16.25), 16.0);
        // 16.25 is not a half at all -- round() there is ordinary nearest-integer, checked
        // above only because it is one of the shipped defaults' own products. The exact
        // halves below are round_half_to_even()'s real target.
        assert_eq!(round_half_to_even(6.5), 6.0);
        assert_eq!(round_half_to_even(9.5), 10.0);
        assert_eq!(round_half_to_even(12.5), 12.0);
        assert_eq!(round_half_to_even(0.5), 0.0);
        assert_eq!(round_half_to_even(1.5), 2.0);
    }
}
