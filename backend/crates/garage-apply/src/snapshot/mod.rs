//! `make_snapshot()`: the whole live state the QML client draws every pane from, in one call.
//!
//! One JSON object, assembled from the preferences plus eight live reads: the resolved
//! default applications, the display arrangement, the workspace allocation over it, the
//! resolved shortcut set, audio, the date and time, the region, and whether a touchpad is
//! attached -- plus a capability map (which of `hyprctl`, `hyprpaper`, `hyprsunset`, `pactl`
//! and `hypridle` are even on the machine) and an error string carrying whatever
//! `load_preferences()` could not read.
//!
//! Preferences are reported exactly as stored, not with the wallpaper resolved to whatever
//! `current` points at -- that used to happen, and it meant the pane was shown a path nothing
//! had ever written: after picking a solid colour it read back the generated
//! `solid-1c1c1e.png` as though that were the chosen picture.
//!
//! See the submodules for each live section: [`display`], [`workspaces`], [`audio`],
//! [`input`], [`apps`], [`datetime`] and [`region`] are the reads `make_snapshot()` composes;
//! [`keybindings`] is the resolved shortcut set alongside them. All doc-only: every function
//! here returns a snapshot value for the JSON envelope, not a `Result<(), ApplyError>`, and
//! none of them is reached through `Route::steps()` -- `snapshot` is its own top-level
//! command in `main()`, not a route.

mod apps;
mod audio;
mod datetime;
pub(crate) mod display;
mod input;
mod keybindings;
mod region;
mod workspaces;
