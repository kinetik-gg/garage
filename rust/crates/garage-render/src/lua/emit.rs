//! The Lua table bodies shared between a generated fragment and a live `hyprctl eval`.
//!
//! Four builders -- `glass_options()`, `material_decoration()`, `border_general()` and
//! `motion_lua()` -- plus `lua_pairs()`, the `", "`-joining helper all four use to turn a
//! `role = value` map into the inside of a Lua table constructor. Each builder is called
//! from two places in the Python: once from the renderer that writes the value into
//! `hyprland.lua`'s generated fragment, and once from the applier that pushes the same value
//! into the running compositor with `hyprctl eval`. Sharing the builder is what keeps a
//! reload and a live push from ever disagreeing about what a setting means -- there is
//! exactly one place that turns `glass_blur = "heavy"` into Lua, and both the file and the
//! live session read it.
//!
//! `glass_options()` is not a separate code path for "frosted": a bevel steepness of zero
//! leaves the surface flat, so the lateral shift is zero and the same shader produces plain
//! blur, while edge clarity and the rim light stay independent of the refraction vector and
//! so still read correctly on a flat surface. `material_decoration()` exists because
//! Hyprland's own blur and the window opacity are separate code paths from the Kinetik Glass
//! plugin -- disabling the plugin alone used to leave both still running, which is the
//! opposite of solid, so the core decoration options are driven from here instead.
//! `border_general()` folds the border colour in with its size rather than leaving the
//! colour a setting of its own, because `decorations.lua` paints both borders fully
//! transparent and a size with no colour would silently shrink the window and draw nothing.
//! `motion_lua()` switches animations off outright for Reduce Motion rather than shortening
//! them -- a shorter slide is still a slide, which is the thing the setting exists to
//! remove -- and keeps emitting per-leaf speeds even while motion is off, so switching it
//! back on restores the desktop without a second push.
//!
//! `corner_rounding()` lives here too, alongside the builders rather than with
//! [`crate::corner`]: windows, layer surfaces, hyprexpo's overview tiles and the shell's own
//! QML silhouettes are four separate code paths that all have to be handed the same rounding
//! in px, and this is the one function every one of `preferences.rs`'s and `corner.rs`'s Lua
//! emits it through.
//!
//! Doc-only, for the reason given in [`crate::lua::escape`]: every function here returns
//! `String` or `int`, not a write outcome, so a `pub(crate)` stub with this crate's fixed
//! `(cx) -> Result<(), RenderError>` shape would be a signature Phase 3 has to throw away
//! rather than fill in.
