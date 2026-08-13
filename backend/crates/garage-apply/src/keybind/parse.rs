//! `parse_combination()` and its derived spellings: splitting and comparing a bind string.
//!
//! Splitting on `+` is safe because the key that character stands for is spelled `"plus"` in
//! every keysym table this reads against; a bare `+` on its own is an empty part, and empty
//! parts are refused outright.
//!
//! Three spellings come out of the same parse, each for a different reader:
//! `canonical_combination()` is the pane's own list spelling -- case is the writer's taste,
//! since the compositor matches keysyms case-insensitively, so storing whichever case arrived
//! would show one shortcut two ways in the Keybindings pane. `combination_id()` strips
//! whitespace and lowercases, and must agree exactly with `bind_id()` in `config/binds.lua`,
//! which is the side that hands ids out and what `hl.unbind` matches on. `combination_signature()`
//! is what the compositor actually matches on for telling two binds apart -- an unordered set
//! of modifiers plus a lowercase key -- because modifiers reach Hyprland as a mask, so
//! `SUPER+SHIFT+A` and `shift + super + a` are one shortcut and a set holding both would fire
//! both handlers off a single press.
//!
//! `require_bindable()` checks against the canonical spelling, not the one that arrived --
//! what is stored and handed to the compositor is the canonical form, so checking anything
//! else would refuse `"f5"` while allowing the `"F5"` it normalises to. It also refuses a
//! bare key with no modifier unless that key is in the small set safe to bind standalone,
//! because a bare `F5` would be swallowed everywhere rather than reaching this shortcut.
//!
//! Doc-only: every function here parses or compares a string, returning a parsed value or
//! `bool`, not `Result<(), ApplyError>`.
