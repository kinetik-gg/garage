//! `rofi_palette_rasi()`: the launcher palette. `rasi` spells alpha as two hex digits
//! appended to the colour.
//!
//! A handful of roles handed straight through -- background, selection, border and two text
//! weights -- each with the alpha rofi's `rasi` syntax wants baked into the hex string itself
//! rather than a separate channel, and a `background-color` deliberately left transparent so
//! the base theme's own layer shows through. `@import "apple-base.rasi"` carries everything
//! that is not a colour, so this file's whole job is naming a few variables.
//!
//! Doc-only: returns a `String`, written by `render_toolkits()` rather than by this module.
