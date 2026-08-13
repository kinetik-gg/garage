//! `PALETTE`: the two schemes' semantic roles, and the vocabulary every toolkit writer reads.
//!
//! Two dictionaries, `"dark"` and `"light"`, each mapping a role name -- `bg`, `fg_strong`,
//! `accent`, `border_active`, and around fifty more -- to a colour. Most roles are opaque
//! `#rrggbb`; some are composited `rgba(r, g, b, a)`, for a surface meant to let the glass or
//! the wallpaper show through. [`crate::theme`]'s `opaque()` is what refuses to hand a
//! composited role to a toolkit that can only spell an opaque hex, so the failure happens
//! where this table is rather than silently on the next login.
//!
//! Bodies run sunken to floating (`bg_sunken` .. `bg_lifted`), text runs loudest to faintest
//! (`fg_bright` .. `fg_faint`), and `on_bg` is the one colour that is never itself a body: it
//! is the tint direction, white on dark and black on light, and every hairline, sheen and
//! hover state is a fractional alpha of it rather than a colour of its own. Signal colours --
//! `ok`, `warn`, `danger`, `info`, `caution`, `violet` -- are shared by both schemes on
//! purpose: a temperature graph and a failed unlock mean the same thing whatever the desktop
//! looks like.
//!
//! Doc-only: this is a data table, not a renderer. Its Rust shape -- a `const` map, a
//! generated lookup, or something else -- is a Phase 3 decision this scaffold does not make.
