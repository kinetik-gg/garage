//! `apply_display_layout()`: check a candidate layout's geometry, then render and reload it.
//!
//! Every enabled, non-mirrored display's rectangle is checked against every other's: no two
//! may overlap, and the whole set must be edge-to-edge connected -- reachable from the first
//! display by crossing only touching edges -- or the layout would contain a gap no window
//! could ever be dragged across. A mirror is excluded from both checks entirely, because
//! Hyprland stacks it exactly on its source, which would otherwise read as a total overlap,
//! and it touches no edge of its own to connect through.
//!
//! Touching is judged with a half-pixel tolerance on each edge, which is what lets two
//! displays of different native scales still register as adjacent despite floating-point
//! rounding in their scaled widths and heights.
//!
//! Only once the geometry is accepted does this render the fragment and reload the
//! compositor -- checked before written, so a rejected layout never reaches
//! `hyprland.lua` at all.
//!
//! Doc-only: operates on a display-layout value and reports a checked/reloaded outcome, not
//! `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx), and is reached from
//! [`crate::displays::transaction`] rather than being a dispatch target itself.
