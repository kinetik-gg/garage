//! `stow_state()`, `dangling_repo_links()` and `managed_paths()`: what stow has and has not
//! placed, and the links a version bump left pointing at nothing.
//!
//! `managed_paths()` walks the checkout's `desktop/` tree the way `stow --no-folding` would,
//! against the path-anchored patterns in `.stow-local-ignore` -- only the anchored ones,
//! since the rest of that file repeats stow's own name-based defaults, which are applied by
//! name rather than by path. An anchored pattern excludes the toolkit configs Garage rewrites
//! at runtime, and a manifest that included them would report every theme switch as a broken
//! link.
//!
//! `stow_state()` reports five outcomes per managed path: `linked` (a symlink into this
//! checkout that resolves -- healthy), `other` (a stow link at the right relative path inside
//! a *different* checkout -- the desktop works, it is just served from elsewhere, which is
//! what a moved or duplicated clone looks like and what a restow corrects), `broken` (a link
//! into this checkout whose target is gone, or a link to something else entirely), `plain` (a
//! real file sitting where a link belongs, which bootstrap backs up), and `missing` (nothing
//! there at all, what a file added since the last stow looks like).
//!
//! `dangling_repo_links()` finds the gap no restow can close: a file deleted from the
//! repository between two versions is not in the *current* managed set, so a plain rescan
//! never considers the stale link it left behind, and `stow --restow` only unlinks what the
//! package still contains today. This walks the other direction instead -- scan `$HOME`'s
//! known Garage-managed roots, keep the links that point into this checkout and no longer
//! resolve -- which needs no record of what a previous version shipped. Scoped tightly to
//! four roots on purpose: `$HOME` is full of symlinks that dangle legitimately (Chrome's
//! `SingletonLock`, Discord's IPC sockets, editor session files), and a sweep considering
//! those would eventually delete one.
//!
//! Doc-only: every function here inspects the filesystem and returns a report value, not
//! `Result<(), ApplyError>` over a [`SessionCx`](crate::cx::SessionCx).
