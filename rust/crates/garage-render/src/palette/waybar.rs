//! `waybar_style_css()`: the bar's whole generated stylesheet -- colours, the base import,
//! the scaled spacing.
//!
//! Its own function, and not just tidiness: `bar.padding_scale` is a slider, so `set` fires
//! one of these per notch, and the narrow apply-side `render_bar_style()` writes this one
//! file alone rather than dragging the other twenty toolkit configs
//! [`crate::palette::toolkits`] owns along with it.
//!
//! Waybar uses Hyprland's native layer blur, with a light or dark translucent tint selected
//! from the resolved scheme -- [`crate::bar::style`]'s `bar_background()` is where that tint
//! is picked. The bar's foreground is the tint direction itself, `on_bg` from
//! [`crate::palette::table`] -- white over the dark glass, black over the light -- because it
//! is read against the desktop rather than against a surface. The workspace dot's three
//! opacity states (idle, hover, active) are the one place this file spells out an alpha ramp
//! of its own rather than reading one from the palette, because none of the three is a role
//! any other surface needs.
//!
//! Composed from [`crate::bar::style`]'s `bar_background()` and [`crate::bar::spacing`]'s
//! `waybar_spacing_css()`, appended after the generated colour block and the `@import` of
//! `waybar-base.css`.
//!
//! Doc-only: returns a `String`, written both by `render_toolkits()` (every render) and by
//! `garage-apply`'s narrow `render_bar_style()` (a padding or background change alone).
