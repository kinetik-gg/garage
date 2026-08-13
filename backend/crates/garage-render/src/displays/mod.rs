//! The saved display layout: the value both halves read, and `render_displays()`.
//!
//! # Why the reader is here and not on the apply side
//!
//! `load_display_config()` and `mirror_targets()` are read by four callers that do not all
//! live in one crate: `render_displays()` and `workspace_outputs()` here, and
//! `apply_display_layout()`, `normalize_display_layout()`, `layout_toml()` and
//! `display_snapshot()` in `garage-apply`. `garage-apply` depends on `garage-render` and
//! never the other way round, so the shared half belongs at the bottom of that edge -- the
//! lowest crate both can name. This is the same placement, for the same reason, as
//! [`crate::keybinds`]'s `Document`: the *shape* of a user-owned file lives beside the
//! renderer that consumes it, while every rule about what may go into it lives in the crate
//! that may act on the session.
//!
//! # Leniency, which is not a shortcut
//!
//! Everything here is lenient, unlike the apply-side layout check. This also runs at session
//! start from a possibly hand-edited `displays.toml`, and refusing the whole fragment because
//! one monitor rule is bad would drop *every* monitor rule, not just the offending one --
//! leaving the machine on the catch-all. So a display that names an impossible mirror
//! (itself, a disabled output, another mirror) simply mirrors nothing here, and a VRR value
//! Hyprland would reject is clamped into `-1..3` rather than refused.
//! [`strict_mirror_targets`] is the same resolution with the refusals turned back on, for the
//! apply path, which has to reject those combinations rather than emit them: Hyprland's own
//! `setMirror` logs and ignores exactly those cases, so the layout on screen would otherwise
//! quietly stop matching the one that was saved.
//!
//! # What is written, and what is deliberately not
//!
//! Mirror sources are written ahead of the mirrors that name them. Hyprland collects every
//! rule and applies them once the whole config has parsed -- verified by generating the mirror
//! line first, which mirrored just the same -- so the ordering makes no difference to the
//! compositor; it is for whoever reads the generated file by hand.
//!
//! A mirror is written with no position of its own: Hyprland drops a mirrored output out of
//! the monitor layout entirely and pins it onto its source, so a coordinate would only be a
//! stale value fighting that. The one it keeps in `displays.toml` is where it goes back to
//! when the mirror is turned off.

mod read;
mod render;
mod value;

pub use read::{load_display_config, mirror_targets, strict_mirror_targets};
pub use render::{display_fragment_text, render_displays, render_saved_displays};
pub use value::{DisplayEntry, DisplayLayout, LayoutValue, MirrorRefusal, NumberError};
