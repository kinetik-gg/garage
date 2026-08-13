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
