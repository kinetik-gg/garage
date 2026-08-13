//! `workspaces_snapshot()`: the allocation the pane draws, for the displays now attached.
//!
//! Resolved here rather than in QML, so the pane and the generated fragment cannot disagree
//! about which display owns which ids: there is one allocator (see
//! [`garage_render::workspaces::blocks`]), and this is a read of it. The per-display groups
//! are reported even in shared mode, so the pane can show what switching back to per-display
//! would give without the user having to switch modes to find out.
//!
//! Doc-only: returns a snapshot value for [`crate::snapshot`]'s JSON envelope, not
//! `Result<(), ApplyError>`, and is reached from `make_snapshot()` rather than from
//! `Route::steps()`.
