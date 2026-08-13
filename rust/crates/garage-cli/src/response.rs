//! `response()` and `USAGE`: the envelope every settings-backend command prints, and the
//! help text `garage help` prints verbatim.
//!
//! `response()` is one line of compact JSON -- `{"ok", "data", "error"}` -- printed to
//! stdout by every command except the three plumbing ones
//! ([`garage_apply::doctor`], [`garage_apply::repair`], [`garage_apply::update`], which print
//! lines for a person rather than JSON for the QML client). `ok` is simply `not error`, so a
//! caller never has to reconcile the two fields disagreeing; `data` is `null` for a command
//! that only signals success (`render`, `apply`, `action`) and the actual payload for one
//! that answers a question (`snapshot`, `set`, `display-test`, `theme-sync`). Separators are
//! trimmed to `(",", ":")` because this is read by a JSON parser on the other end, never by a
//! person, and the QML client's own stdout channel is not free.
//!
//! `USAGE` is the text `garage help`, `garage -h` and `garage --help` all print unchanged,
//! and the only place command names and their arguments are written down together for a
//! person to read. It is split into two groups matching the two kinds of command this binary
//! has: the human commands (`doctor`, `repair`, `update`, `help`), which print lines and
//! never go through `response()`, and the settings backend, which always prints exactly one
//! JSON object.
//!
//! Doc-only: `response()`'s shape is a formatting concern layered over whatever `main.rs`'s
//! dispatch produces, not a `Result<(), ApplyError>`-shaped stub -- it is reached from
//! [`crate::commands`]' dispatch, once per command, after the command itself has already
//! succeeded or failed.
