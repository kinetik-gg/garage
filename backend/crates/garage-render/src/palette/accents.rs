//! `ACCENTS` and `BORDER_COLORS`: the choosable accent swatches, and the window border pair
//! they are not part of.
//!
//! `ACCENTS` is the nine accents the Appearance pane offers, in the order it draws the
//! swatches, each with the hex it is drawn as. A different axis from [`crate::palette::table`]
//! entirely: the resolved scheme decides the bodies and the text, this decides the one colour
//! the user gets to pick on top of it. The names are GNOME's, because `push_accent()` hands
//! them straight to `org.gnome.desktop.interface accent-color`, and the hexes are the ones
//! GNOME itself draws for them so a GTK app and the shell agree on what "teal" is.
//! (`ACCENTS`, garage:575-596)
//!
//! Not a second table: `garage_core::schema::enums::AccentColor` already carries this exact
//! mapping as an exhaustive `const fn` match (`AccentColor::hex()`, ported from the same
//! source lines in task 2.5, before this module existed). Duplicating it here as data would
//! recreate precisely the failure mode `PALETTE`'s own header describes -- two copies of one
//! identity, free to drift -- so [`ACCENTS`] below is a thin re-export, generated from
//! `AccentColor::ALL` rather than typed out a second time.
//!
//! `BORDER_COLORS` is the active/inactive edge pair the palette draws a window border with,
//! in the `rgba(rrggbbaa)` spelling Hyprland's parser wants, derived from
//! [`crate::palette::table::PALETTE`] rather than chosen independently. Shared with the group
//! borders `render_preferences()` writes, which are the same idea one level in: a group's tab
//! strip already means "this edge is focused", so a bordered window that spoke a different
//! colour would read as a second, unrelated convention. Keyed by
//! `garage_core::schema::enums::Scheme` here rather than by a bare string, which is what the
//! source's `{scheme: ... for scheme, colors in PALETTE.items()}` comprehension actually
//! indexes by -- not by `AccentColor`, a different axis entirely. (`BORDER_COLORS`,
//! garage:599-608)

use crate::palette::table;
use garage_core::schema::enums::{AccentColor, Scheme};

/// The nine accents in Appearance-pane order, each paired with the hex
/// [`AccentColor::hex()`] draws it as.
#[cfg_attr(
    not(test),
    // Not the toolkit emitters after all: the Python reads `ACCENTS` in exactly one place
    // outside the pane, `PREFERENCE_SCHEMA`'s `"members": set(ACCENTS)` (garage:988-991), so
    // the swatch list and the hexes drawn for them cannot come apart. `render_toolkits()`
    // never touches it -- the accent reaches the desktop through `push_accent()` and the
    // accent marker, both on the apply side.
    expect(
        dead_code,
        reason = "the swatch list PREFERENCE_SCHEMA's members are drawn from"
    )
)]
pub(crate) const ACCENTS: [(&str, &str); 9] = [
    (AccentColor::Blue.as_str(), AccentColor::Blue.hex()),
    (AccentColor::Teal.as_str(), AccentColor::Teal.hex()),
    (AccentColor::Green.as_str(), AccentColor::Green.hex()),
    (AccentColor::Yellow.as_str(), AccentColor::Yellow.hex()),
    (AccentColor::Orange.as_str(), AccentColor::Orange.hex()),
    (AccentColor::Red.as_str(), AccentColor::Red.hex()),
    (AccentColor::Pink.as_str(), AccentColor::Pink.hex()),
    (AccentColor::Purple.as_str(), AccentColor::Purple.hex()),
    (AccentColor::Slate.as_str(), AccentColor::Slate.hex()),
];

/// The active/inactive window-border edge pair for one scheme, as `rgba(rrggbbaa)`. Derived
/// from `PALETTE`'s `border_active` / `border_inactive` roles rather than chosen
/// independently, so the window border and the locked-group tab border can never drift from
/// each other.
///
/// `border_active` and `border_inactive` are two of [`table::PALETTE`]'s fixed roles, present
/// for both schemes by construction (`palette::parity::every_role_is_defined_for_both_schemes`
/// pins this against the Python source); their absence here would be a broken build the
/// palette-parity test already catches, not a condition a running renderer could hit.
///
/// # Panics
///
/// Never, for the reason the paragraph above gives: both roles are defined for both schemes
/// by construction, and the palette-parity test is what holds that true.
#[must_use]
pub fn border_colors(scheme: Scheme) -> (String, String) {
    #[allow(clippy::expect_used)]
    let active = table::role(scheme, "border_active").expect("border_active is a PALETTE role");
    #[allow(clippy::expect_used)]
    let inactive =
        table::role(scheme, "border_inactive").expect("border_inactive is a PALETTE role");
    (border_rgba(active), border_rgba(inactive))
}

/// `#rrggbb` as the `rgba(rrggbbaa)` spelling Hyprland's parser wants, alpha pinned opaque.
fn border_rgba(hex: &str) -> String {
    format!("rgba({}ff)", hex.trim_start_matches('#'))
}
