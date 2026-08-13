//! `ACCENTS` and `BORDER_COLORS`: the choosable accent swatches, and the window border pair
//! they are not part of.
//!
//! `ACCENTS` is the nine accents the Appearance pane offers, in the order it draws the
//! swatches, each with the hex it is drawn as. A different axis from [`crate::palette::table`]
//! entirely: the resolved scheme decides the bodies and the text, this decides the one colour
//! the user gets to pick on top of it. The names are GNOME's, because `push_accent()` hands
//! them straight to `org.gnome.desktop.interface accent-color`, and the hexes are the ones
//! GNOME itself draws for them so a GTK app and the shell agree on what "teal" is.
//!
//! `BORDER_COLORS` is the active/inactive edge pair the palette draws a window border with,
//! in the `rgba(rrggbbaa)` spelling Hyprland's parser wants, derived from `PALETTE` rather
//! than chosen independently. Shared with the group borders `render_preferences()` writes,
//! which are the same idea one level in: a group's tab strip already means "this edge is
//! focused", so a bordered window that spoke a different colour would read as a second,
//! unrelated convention.
//!
//! Doc-only: both are data derived from [`crate::palette::table`], not renderers of their own.
