//! Quoting a string as a Lua string literal -- `lua_string()`.
//!
//! Not `serde_json`: JSON and Lua only look alike. JSON's default `\uXXXX` escape for
//! non-ASCII is a syntax error in Lua, which spells the same thing `\u{XXXX}`, and Lua
//! rejects a raw newline inside a quoted literal where JSON never has one to reject.
//! Non-ASCII is emitted as UTF-8 rather than escaped: the fragment is written UTF-8 and Lua
//! takes string literals as bytes, so the two are the same string and the generated file
//! stays readable.
//!
//! The escape table itself is short on purpose: the two characters that end a Lua `"..."`
//! literal early (`\` and `"`), plus the whitespace Lua's lexer treats as a line break
//! inside one (`\n`, `\r`, `\t`). Everything else printable passes through verbatim, and
//! anything below `0x20` or `0x7f` is escaped braced -- `\u{XXXX}` rather than Lua's decimal
//! `\ddd` -- so the escape cannot run into a literal digit that follows it in the source.
//!
//! Doc-only: this crate's stub convention gives a module one `pub(crate)` entry point per
//! renderer that writes a file and reports `Result<(), RenderError>`. `lua_string()` and its
//! neighbours return `String`, not a write outcome, so they stay documented rather than
//! stubbed with a signature Phase 3 would only have to undo.
