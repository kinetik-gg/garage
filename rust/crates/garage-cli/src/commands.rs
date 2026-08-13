//! `main()`'s dispatch shape: which command name reaches which call, and the three that skip
//! the JSON envelope entirely.
//!
//! Command resolution happens once, ahead of everything else: `argv[1]`, defaulting to
//! `"snapshot"` when the binary is run with no arguments at all, which is what lets the QML
//! client's simplest possible invocation -- no arguments -- ask for the whole live state.
//! `help`, `-h` and `--help` are recognised before any preferences file is touched, so `garage
//! help` never fails even on a machine with no config directory yet.
//!
//! The three plumbing commands -- `doctor`, `repair`, `update` -- are dispatched from a
//! separate table checked before the JSON path, and their errors go to stderr as plain text
//! rather than through [`crate::response`]'s envelope; see [`garage_apply::doctor`] for why.
//! Every other command runs `migrate_config_root()` once, ahead of its own dispatch rather
//! than inside a loader, because an action like `keybind.rebind` or `display-test` reaches
//! `keybindings.toml` or `displays.toml` directly without ever loading the preferences, and
//! either one arriving first at the old config layout would write a fresh file at the new
//! path while the user's own sat at the old one.
//!
//! Fifteen command names in the settings-backend table: `snapshot`, `render`, `render-idle`,
//! `render-bar`, `render-wallpaper`, `apply`, `set`, `action`, `display-test`,
//! `display-confirm`, `display-revert`, `_display-watchdog` (unlisted in `USAGE`, since it is
//! the watchdog's own re-entry point and not something a person types), `theme-sync` and
//! `night-shift-sync`. Each is a thin call into [`garage_apply`] or
//! [`garage_render`], wrapped in [`crate::response::response`] except the watchdog, which
//! runs unattended and has nobody to report to.
//!
//! Doc-only: `main()`'s real dispatch takes `argv: &[String]` and returns an exit code, which
//! is not a shape this crate's other stub conventions (`RenderCx`/`SessionCx`-shaped
//! functions) apply to -- `main.rs` itself stays a no-op `fn main() {}` until Phase 3 wires
//! this dispatch in.
