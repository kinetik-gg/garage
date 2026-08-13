//! `load_display_config()`, `normalize_display_layout()` and `layout_toml()`: reading,
//! normalising and serialising `displays.toml`.
//!
//! `mirror_targets()` lives here too, in two modes: lenient for
//! [`garage_render::displays`]'s renderer, which also runs at session start from a possibly
//! hand-edited file and must not let one bad mirror rule take every monitor rule down with
//! it, and strict for [`crate::displays::apply`], which has to refuse a mirror that names
//! itself, a disabled output, or another mirror rather than silently dropping the rule --
//! Hyprland's own `setMirror` logs and ignores exactly those cases, so the layout on screen
//! would otherwise quietly stop matching the one that was saved.
//!
//! `normalize_display_layout()` re-anchors the arrangement at `(0, 0)`, from the outputs that
//! actually occupy the desktop -- a mirror only carries a parked coordinate for when it is
//! turned off again, and anchoring on that would slide every real display away from the
//! origin.
//!
//! Doc-only: these operate on a display-layout value, not a
//! [`SessionCx`](crate::cx::SessionCx), and are reached from [`crate::displays::transaction`]
//! and [`crate::displays::apply`] rather than being dispatch targets themselves.
