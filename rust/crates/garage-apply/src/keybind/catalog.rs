//! `read_keybind_catalog()`: the published bind set, and whether it can be proved whole.
//!
//! Read rather than derived. `hyprctl binds -j` reports where a bind ended up and an opaque
//! Lua registry index, never the combination the file wanted -- and the two differ in exactly
//! the case there is something worth showing. Parsing `binds.lua` from here would be a
//! second, worse Lua interpreter that would have to know what the workspace loop expands to.
//!
//! # The fail-closed witness-line contract
//!
//! Every line of the published catalog is self-describing, so a truncated file reads back as
//! a complete but shorter one, and nothing in the individual entries marks it as a fragment.
//! That is precisely how a reader that filtered the user's overrides against it once came to
//! delete overrides it simply had not seen yet, mid-publication. So `config/binds.lua` writes
//! a witness line last -- `#end<TAB>N`, with `N` the number of rows -- and `read_keybind_catalog()`
//! reports the catalog as unverified unless the file both ends with that line and `N` matches
//! the number of rows actually parsed. Fail-closed: any tear, any short count, any missing
//! witness leaves the catalog unverified, and every caller downstream may still read it and
//! list it, but must never conclude from it that a shortcut the catalog does not mention has
//! ceased to exist.
//!
//! False does not mean broken. A session still running a copy of `binds.lua` that shipped
//! before the witness line existed publishes a perfectly good catalog with no last line, and
//! it must keep working for everything except that one conclusion.
//!
//! `keybind_catalog()` is the plain reader built on top, for callers that only want the
//! default set and do not need the verified flag -- [`crate::snapshot::keybindings`]'s
//! snapshot is one. `resolve_keybinds()` is the whole bind set `config/binds.lua` would
//! register from a document: each catalog entry keeps its default unless it is overridden and
//! not protected, plus every custom shortcut appended after it.
//!
//! Doc-only: reads a file and returns catalog/document values, not `Result<(), ApplyError>`.
