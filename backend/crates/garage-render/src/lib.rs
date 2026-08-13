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
mod displays;
pub mod error;
mod general;
mod idle;
mod keybinds;
mod lua;
mod motion;
mod palette;
mod preferences;
mod region;
#[cfg(test)]
mod render_parity_tests;
mod search;
pub mod theme;
mod wallpaper;
mod workspaces;

pub use cx::RenderCx;
pub use error::RenderError;
