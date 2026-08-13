//! `display_snapshot()`: every monitor Hyprland reports, folded with the saved layout.
//!
//! Queries `hyprctl monitors all` rather than the default list: Hyprland pulls a mirrored
//! output out of its monitor layout entirely -- it loses its `wl_output` too, so `grim`
//! cannot see it either -- and a disabled one never appears there either. Both would vanish
//! from the pane with no control left to turn them back on if the default list were used
//! instead.
//!
//! `mirrorOf` is reported as the source's *id*, rendered as a string, with the literal
//! `"none"` for a display that mirrors nothing; resolving it through an id-to-name map is
//! what turns that back into an output name. `mirrorOf` is never tested for truthiness, only
//! for the literal `"none"` -- id `0` is a real monitor, and truthiness would misread it as
//! absent.
//!
//! A mirror is reported at its source's position, so the spot it holds in the arrangement
//! only survives in `displays.toml` -- the snapshot substitutes the saved position back in
//! for a mirrored output, or turning the mirror off would drop the display on top of the very
//! output it was copying.
//!
//! If nothing in the result is marked primary, the focused display is promoted to primary
//! (falling back to the first) so the pane always has exactly one primary to show, even on a
//! machine that has never saved a layout.
//!
//! Doc-only: `display_snapshot()` returns a snapshot value for [`crate::snapshot`]'s JSON
//! envelope, not `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than
//! from `Route::steps()`.
