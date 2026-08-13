//! `swayosd_palette_css()`: the volume/brightness OSD palette, as GTK named colours.
//!
//! Six roles -- the OSD's edge, body, foreground, muted foreground, track and fill -- each a
//! `@define-color`, imported by a small per-scheme `style.css` alongside `swayosd-base.css`.
//! The OSD is the one surface that reads its own composited body colour directly rather than
//! through a toolkit's own theming layer, which is why it gets a palette writer of its own
//! rather than sharing GTK's.
//!
//! Doc-only: returns a `String`, written by `render_toolkits()` rather than by this module.
