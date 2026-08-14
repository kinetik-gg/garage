//! Moves the running session to match desired state.
//!
//! The other half of the split. Where a render writes a file and stops, an apply is what
//! makes the desktop agree with it: `hyprctl reload` and `hyprctl eval`, `gsettings`,
//! `pkill -HUP xsettingsd`, `systemctl --user restart`, `loginctl lock-session`. Most of
//! the Python's pairs are named for it -- `render_theme()`/`push_theme()`,
//! `render_accent()`/`push_accent()` -- and `apply_preferences()` is where the two halves
//! are put back together.
//!
//! The direction is one-way and enforced by the types: [`cx::SessionCx`] contains a
//! `RenderCx`, so an applier can run any renderer, while a renderer has no field, no
//! dependency edge and no widening operation that would let it reach back. See
//! [`cx::SessionCx`] for what that prevents.
//!
//! Unlike `garage-render`, this crate does depend on `garage-prefs` and `garage-proc`,
//! which is exactly the difference: an apply is allowed to hold the preferences lock and
//! allowed to run programs, because acting on the session is what it is for.
#![forbid(unsafe_code)]

pub mod actions;
mod bar;
mod border;
mod command;
mod corner;
pub mod cx;
pub mod desktopfiles;
pub mod dispatch;
#[cfg(test)]
mod display_parity_tests;
#[cfg(test)]
mod display_trace_tests;
pub mod displays;
pub mod doctor;
pub mod error;
mod eval;
mod file_index;
mod glass;
pub mod keybind;
mod locale;
pub mod migrations;
mod motion;
pub mod night_shift;
mod region;
pub mod repair;
#[cfg(test)]
mod repair_transcripts;
pub mod route;
pub mod snapshot;
pub mod terminal;
#[cfg(test)]
mod testing;
pub mod theme;
pub mod update;
mod wallpaper;
#[cfg(test)]
mod workspace_trace_tests;
mod workspaces;

pub use actions::action;
pub use cx::SessionCx;
pub use doctor::doctor;
pub use error::ApplyError;
pub use night_shift::apply_night_shift;
pub use repair::repair;
pub use route::apply_preferences;
pub use snapshot::make_snapshot;
pub use theme::theme_sync;
pub use update::update;
