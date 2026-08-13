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
//! Doc-only: this returns a `String`, not a write outcome, and is reached only from
//! [`crate::palette::waybar`] and from `garage-apply`'s narrow `render_bar_style()`, neither
//! of which is this module.
