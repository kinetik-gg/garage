//! `bar_background()`: the bar's body colour, as `bar.background` asks for it.
//!
//! `"blurred"` is the palette's `bar_bg` unchanged -- the translucent light or dark tint that
//! gives Hyprland's blur something to frost. `"transparent"` is that same colour at zero
//! alpha, which leaves the blur layer alone behind the bar: the layer rule and its
//! `ignore_alpha` apply to the layer, not to the fill, so the frost survives a body nobody
//! can see.
//!
//! Read from the palette rather than from a second table of alphas, so the tint the bar is
//! drawn with stays one number in one place. The composer that turns this, the workspace dot
//! colour and the rest of the resolved palette into the bar's whole stylesheet is
//! `waybar_style_css()`, in [`crate::palette::waybar`] rather than here -- it is one of the
//! per-toolkit palette writers `render_toolkits()` calls, alongside GTK, Qt, rofi and
//! swayosd's.
//!
//! Returns a `String`, not a write outcome, and is reached only from
//! [`crate::palette::waybar`] and from `garage-apply`'s narrow `render_bar_style()`, neither
//! of which is this module. `waybar_style_css()` is not ported yet, which is why the function
//! below carries a `dead_code` expectation rather than a caller.

use garage_core::schema::enums::BarBackground;
use garage_core::schema::Preferences;

use crate::palette::table::role;

/// The bar's body colour, as `bar.background` asks for it (garage:3833-3853).
///
/// `"blurred"` is `PALETTE`'s `bar_bg` unchanged; `"transparent"` is that same colour at zero
/// alpha, which leaves the blur layer alone behind the bar.
#[must_use]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "consumed by waybar_style_css(), which task 3.7 ports"
    )
)]
pub(crate) fn bar_background(
    scheme: garage_core::schema::enums::Scheme,
    prefs: &Preferences,
) -> String {
    let tint = role(scheme, "bar_bg").unwrap_or_default();
    if prefs.bar.background != BarBackground::Transparent {
        return tint.to_owned();
    }
    // The same three channels at zero alpha. Taken from the tint itself rather than from
    // rgb_parts(), which reads an opaque hex: every composited role is spelled
    // `rgba(r, g, b, a)`, so everything up to the last comma is the body.
    format!("rgba({}, 0.00)", channels(tint))
}

/// `tint[tint.index('(') + 1:tint.rindex(',')]`: the body of an `rgba(...)` spelling, up to
/// but not including its last comma. `bar_bg` is composited in both schemes by construction,
/// so both delimiters are always present; a hex that somehow reached here would yield the
/// empty string rather than a panic, which is the one behaviour Python's slicing and Rust's
/// `find`/`rfind` can be made to share.
fn channels(tint: &str) -> &str {
    let Some(open) = tint.find('(') else {
        return "";
    };
    let Some(last_comma) = tint.rfind(',') else {
        return "";
    };
    tint.get(open + 1..last_comma).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use garage_core::schema::defaults::Defaults;
    use garage_core::schema::enums::Scheme;
    use garage_core::schema::notes::Notes;
    use garage_core::schema::Preferences;

    use super::bar_background;

    fn prefs_from(departures: &str) -> Preferences {
        let table: toml::Table = departures.parse().expect("fixture toml parses");
        let defaults = Defaults::compiled().expect("shipped defaults parse");
        let mut notes = Notes::new();
        Preferences::coerce_from(&table, &defaults, &mut notes)
    }

    #[test]
    fn blurred_is_the_palette_tint_and_transparent_is_that_tint_at_zero_alpha() {
        let blurred = prefs_from("[bar]\nbackground = \"blurred\"\n");
        assert_eq!(
            bar_background(Scheme::Dark, &blurred),
            "rgba(28, 28, 30, 0.42)"
        );
        assert_eq!(
            bar_background(Scheme::Light, &blurred),
            "rgba(245, 245, 247, 0.42)"
        );

        let transparent = prefs_from("[bar]\nbackground = \"transparent\"\n");
        assert_eq!(
            bar_background(Scheme::Dark, &transparent),
            "rgba(28, 28, 30, 0.00)"
        );
        assert_eq!(
            bar_background(Scheme::Light, &transparent),
            "rgba(245, 245, 247, 0.00)"
        );
    }
}
