//! Pure renderers: write files, signal nothing, structurally cannot take the preferences lock.
//!
//! A render reads layers 1 and 2 -- the shipped defaults and the user's own files -- and
//! writes layer 3, the generated fragments and markers under `~/.local/state/garage`.
//! Losing the whole of layer 3 costs one render and no settings, which is the property
//! that makes a renderer safe to run at any moment, from a unit's `ExecStartPre`, from an
//! apply, or by hand.
//!
//! "Signals nothing" is the contract rather than a happy accident: no `gsettings`, no
//! service restart, no `pkill`, no `hyprctl eval` or `reload`. Everything that moves the
//! running desktop is `garage-apply`'s.
//!
//! "Structurally lock-free" is the stronger half. This crate depends on `garage-core` and
//! nothing else of Garage's -- in particular not on `garage-prefs`, which owns
//! `PrefLock`, nor on `garage-proc`, which owns process execution. A renderer here cannot
//! take `PREFERENCES_LOCK` because the code that takes it does not resolve from this
//! crate, and `tests/test_lint.py` fails the build if that edge ever appears. See
//! [`cx::RenderCx`] for the deadlock that rule exists to prevent.
#![forbid(unsafe_code)]

mod accent;
pub mod all;
mod bar;
mod corner;
pub mod cx;
pub mod dispatch;
#[cfg(test)]
mod display_parity_tests;
/// Public because `garage-apply` reads and writes the same saved layout: the layout types,
/// `load_display_config()` and `mirror_targets()` are shared by `render_displays()` here and
/// by `apply_display_layout()`, `normalize_display_layout()`, `layout_toml()` and
/// `display_snapshot()` there. The shared half sits at the bottom of the dependency edge --
/// the lowest crate both can name -- exactly as [`keybinds`]'s `Document` does. Publishing
/// the shape is not publishing the ability to act on it.
pub mod displays;
pub mod error;
mod general;
mod idle;
pub mod keybinds;
pub mod lua;
mod motion;
mod palette;
mod preferences;
mod region;
#[cfg(test)]
mod render_parity_tests;
mod search;
pub mod theme;
mod wallpaper;
#[cfg(test)]
mod workspace_parity_tests;
/// Public because `garage-apply`'s `apply_workspace_plan()` needs the plan itself: it asks
/// for one, compares it against the groups the installed fragment still hands out, and
/// salvages every window across the difference before the reload. Publishing the plan is not
/// publishing the ability to act on it -- everything here still only reads files and writes
/// layer 3 plus the allocator's one sanctioned layer-2 file.
pub mod workspaces;

pub use cx::RenderCx;
pub use error::RenderError;

// The render halves of `garage-apply`'s render-then-push pairs, and the two palette builders
// its narrow appliers need.
//
// Most of this crate is reached from outside through [`dispatch::run_render`], which is the
// route table's own door and is exhaustive over [`RenderStep`](garage_core::schema::routes::
// RenderStep). These are the renderers that have no `RenderStep` of their own, because the
// Python calls them from inside an *applier* rather than from a route: `apply_accent()` is
// `render_accent()` then `push_accent()`, `apply_corner_radius()` is `render_corner_radius()`
// then `push_corner_radius()`, `apply_theme()` is `render_theme()` then `push_theme()`, and
// `apply_region()`, `apply_bar_workspaces()`, `apply_bar_widgets()` and `apply_terminal()`
// are each their own renderer followed by one signal.
//
// Publishing them widens what may be *called*, not what this crate may *do*: every one still
// takes a [`RenderCx`], which carries no runner and no lock, so nothing here gains the
// ability to signal anything by being reachable from a crate that can. That is the same
// argument [`displays`] and [`workspaces`] are already public under.
pub use accent::render_accent;
pub use bar::widgets::render_bar_widgets;
pub use bar::workspaces::render_bar_workspaces;
pub use corner::render_corner_radius;
pub use general::{render_general, BrowserCommand};
pub use palette::accents::border_colors;
pub use palette::toolkits::{look, Look};
pub use palette::waybar::waybar_style_css;
pub use preferences::render_preferences;
pub use region::render_region;
pub use theme::render_theme;
