//! `eval_config()`: set live Hyprland options from a Lua table body, without a reload.
//!
//! Hyprland 0.56 parses its config with Lua, and `hyprctl keyword` refuses to work there --
//! "keyword can't work with non-legacy parsers. Use eval." -- so the only way to reach a live
//! option is `hyprctl eval`. That is enough on its own: options are dereferenced per frame
//! and cached nowhere, so no reload is needed for a new value to be picked up. The renderer
//! has already written the fragment by the time this runs, which is what makes the value
//! survive a real reload or a restart even though this call never touches the file.
//!
//! `eval` sets values but damages nothing -- it does not emit `config.reloaded`, which is the
//! only event the glass plugin repaints on -- so a second call writes
//! `decoration:blur:size` back to its own value. A core option set dynamically schedules the
//! refresh it declares, and blur's forces a full frame on every monitor without re-running a
//! single monitor, window or layer rule. Writing it back to itself keeps the nudge free of
//! any visible side effect. It has to be its own `hl.config()` call: a second `decoration`
//! key in the same table constructor is a plain Lua overwrite, and the options it was meant
//! to accompany would be dropped without any error.
//!
//! Doc-only: this returns a captured subprocess result, not `Result<(), ApplyError>`, and is
//! called from inside [`crate::glass`], [`crate::corner`] and [`crate::border`]'s real
//! implementations rather than being a dispatch target of its own. [`crate::motion`] is the
//! one sibling that deliberately does not use it: its per-leaf speeds are top-level
//! `hl.animation()` calls rather than a single `hl.config()` table, and it needs none of the
//! blur write-back this appends.
