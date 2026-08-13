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

mod actions;
mod bar;
mod border;
mod corner;
pub mod cx;
mod desktopfiles;
pub mod dispatch;
mod displays;
mod doctor;
pub mod error;
mod eval;
mod file_index;
mod glass;
mod keybind;
mod locale;
mod motion;
mod night_shift;
mod region;
mod repair;
pub mod route;
mod snapshot;
mod terminal;
mod theme;
mod update;
mod wallpaper;
mod workspaces;

pub use cx::SessionCx;
pub use error::ApplyError;
