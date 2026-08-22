//! Byte-compatible port of the Python backend's TOML emitter (`toml_value`, `dump_toml`).
//!
//! The Python is a hand-rolled emitter, not a TOML library, and this is a port
//! of *it* rather than of the specification. Where the two differ -- string
//! quoting is JSON's, not TOML's, and they agree on everything a preference
//! value can hold -- the Python wins, because the file this writes is the file
//! the Python build reads and rewrites.
//!
//! That matters more than it looks. `compact_preferences_file()` rewrites
//! preferences.toml only when the emitted document differs from what is stored,
//! precisely so a hand-written file keeps its comments -- "`dump_toml()` cannot
//! carry a comment and rewriting it would cost the user the explanation of why
//! the value is there". A Rust emitter that spelled one float or one escape
//! differently would make that comparison fail forever, and the file would be
//! flattened on the next load.

use std::fmt::Write as _;

use crate::pyrepr::{py_repr, py_str_repr, PyValue};

/// What `toml_value()` refuses, with the message text the Python raises.
///
/// The text is load-bearing rather than cosmetic: `SettingsError` reaches the
/// user as a line on stderr, so it is compared as a string by the tests and by
/// anyone reading a journal from either build.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmitError {
    /// A float that is `inf`, `-inf` or `nan`. TOML has spellings for those;
    /// the Python emitter does not use them, because no preference means it.
    #[error("Non-finite numbers are not supported")]
    NonFinite,
    /// Any value that is not a bool, a string, an int or a finite float. The
    /// payload is the Python `repr()` of the offending value, from `{value!r}`.
    #[error("Unsupported TOML value: {0}")]
    Unsupported(String),
}

/// A value as the backend writes one.
///
/// The scalars are the four `toml_value()` handles. `Array` and `Table` are
/// here because the Python's two halves disagree about them on purpose:
/// `dump_toml()` skips a `dict` or `list` sitting where a scalar belongs, and
/// `toml_value()` -- which other call sites reach directly -- refuses it. Both
/// behaviours need something to act on.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// TOML boolean, lowercase in the output whatever Python's repr says.
    Bool(bool),
    /// TOML integer.
    Int(i64),
    /// TOML float, formatted exactly as `CPython`'s `str(float)` would.
    Float(f64),
    /// TOML string, quoted by the JSON encoder the Python emitter borrows.
    Str(String),
    /// An array. Skipped by [`dump_toml`], refused by [`toml_value`].
    Array(Vec<Value>),
    /// An inline table. Skipped by [`dump_toml`], refused by [`toml_value`].
    Table(Vec<(String, Value)>),
}

impl Value {
    /// Whether `dump_toml()` skips this value: the Python's
    /// `if not isinstance(value, (dict, list))`.
    #[must_use]
    pub fn is_container(&self) -> bool {
        match self {
            Value::Array(_) | Value::Table(_) => true,
            Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::Str(_) => false,
        }
    }

    /// Python's `repr()` of this value, for the "Unsupported TOML value"
    /// message. Containers get the `[1, 2]` / `{'a': 1}` spellings a Python
    /// f-string would produce for a list and a dict.
    fn py_repr(&self) -> String {
        match self {
            Value::Bool(flag) => py_repr(PyValue::Bool(*flag)),
            Value::Int(number) => py_repr(PyValue::Int(*number)),
            Value::Float(number) => py_repr(PyValue::Float(*number)),
            Value::Str(text) => py_repr(PyValue::Str(text)),
            Value::Array(items) => {
                let parts: Vec<String> = items.iter().map(Value::py_repr).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Table(entries) => {
                let parts: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", py_str_repr(key), value.py_repr()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
        }
    }
}

/// One `[section]` of a document: keys in the order they were added.
///
/// A `Vec` rather than a map, because both orderings the Python relies on are
/// insertion order -- `preference_deltas()` builds its result by walking the
/// shipped defaults, so the emitted file comes out in the schema's own key
/// order, and a map that sorted or hashed would throw that away.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Section {
    entries: Vec<(String, Value)>,
}

impl Section {
    /// An empty section.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a key. Repeats are kept, exactly as a repeat of `f"{key} = ..."`
    /// would be -- this type is an emission buffer, not a store.
    pub fn push(&mut self, key: impl Into<String>, value: Value) {
        self.entries.push((key.into(), value));
    }

    /// The keys, in emission order.
    #[must_use]
    pub fn entries(&self) -> &[(String, Value)] {
        &self.entries
    }
}

/// A whole document: sections in the order they were added.
///
/// The Python's `if not isinstance(values, dict): continue` has no counterpart
/// here -- a section is a `Section` by construction, so the case it skips
/// cannot be built.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    sections: Vec<(String, Section)>,
}

impl Document {
    /// An empty document.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a section.
    pub fn push(&mut self, name: impl Into<String>, section: Section) {
        self.sections.push((name.into(), section));
    }

    /// The sections, in emission order.
    #[must_use]
    pub fn sections(&self) -> &[(String, Section)] {
        &self.sections
    }
}

/// One value, rendered the way `toml_value()` renders it.
///
/// Booleans are lowercase words rather than Python's `True`/`False`. Strings go
/// through the JSON encoder with `ensure_ascii=False`, which is what the Python
/// calls and is *not* TOML's own escaping -- see [`json_string`]. Numbers are
/// `str(value).lower()`: for an int that is its digits, for a float it is
/// `CPython`'s `repr`, and the `.lower()` is a belt-and-braces no-op once the
/// non-finite values that could have produced an `Inf` are already refused.
///
/// # Errors
///
/// [`EmitError::NonFinite`] for `inf`/`-inf`/`nan`, and
/// [`EmitError::Unsupported`] for an array or a table.
pub fn toml_value(value: &Value) -> Result<String, EmitError> {
    match value {
        Value::Bool(flag) => Ok(if *flag { "true" } else { "false" }.to_string()),
        Value::Str(text) => Ok(json_string(text)),
        Value::Int(number) => Ok(number.to_string()),
        Value::Float(number) => {
            if number.is_finite() {
                Ok(crate::pyrepr::py_float_repr(*number))
            } else {
                Err(EmitError::NonFinite)
            }
        }
        Value::Array(_) | Value::Table(_) => Err(EmitError::Unsupported(value.py_repr())),
    }
}

/// `json.dumps(value, ensure_ascii=False)`, which is how the Python emitter
/// quotes a string.
///
/// `CPython`'s `json.encoder.encode_basestring` escapes exactly six characters by
/// name -- `\` `"` `\b` `\f` `\n` `\r` `\t` -- and everything else below U+0020
/// as `\u00xx` with lowercase hex. Note what it does *not* touch, all of which
/// this has to leave alone too:
///
///   * U+007F (DEL) and the C1 controls go through verbatim. Python's `repr`
///     would escape them; the JSON encoder does not, and this is the JSON
///     encoder.
///   * `ensure_ascii=False` means no `\uXXXX` for non-ASCII at all, so a
///     wallpaper path with an accent in it stays readable in the file.
///
/// So unlike [`py_str_repr`] there is no Unicode table involved and no parity
/// gap: the rule is "below U+0020, or one of two ASCII characters".
pub(crate) fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            // A `fmt::Write` on a `String` cannot fail, so the result is dropped
            // rather than propagated into a signature that has nowhere to put it.
            _ if ch < '\u{20}' => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// `json.dumps(value, indent=indent)`: `CPython`'s pretty-printer, for the generated
/// JSON documents (the bar's watched markers among them)
/// that a handful of renderers write from a [`Value::Table`]/[`Value::Array`] tree built in
/// the order the Python's own `dict` literal or `list.append()` calls would produce it --
/// `Value::Table`'s `Vec<(String, Value)>` is insertion-ordered for exactly this reason.
///
/// Every container is spelled multi-line, one member per line indented `indent` spaces per
/// nesting level, with `", "`'s comma alone (no trailing space, since the newline follows it)
/// between members and `": "` between an object key and its value -- `CPython`'s own
/// separators once `indent` is not `None`. An empty array or object is the one exception,
/// kept inline as `[]`/`{}` rather than opening onto an empty line, which is also what
/// `CPython` does. Strings are quoted through [`json_string`], the same escaper
/// [`toml_value`] uses for a TOML string, since both are `json.dumps`'s own encoder.
///
/// Every caller of this wants an object at the top, so nothing here appends the trailing
/// newline `json.dumps(...) + "\n"` adds in every call site the Python has -- that belongs to
/// the caller, alongside its own header comment if it has one.
#[must_use]
pub fn json_dumps(value: &Value, indent: usize) -> String {
    let mut out = String::new();
    write_json(value, indent, 0, &mut out);
    out
}

fn write_json(value: &Value, indent: usize, level: usize, out: &mut String) {
    match value {
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Int(number) => {
            let _ = write!(out, "{number}");
        }
        Value::Float(number) => out.push_str(&crate::pyrepr::py_float_repr(*number)),
        Value::Str(text) => out.push_str(&json_string(text)),
        Value::Array(items) => {
            let members = items.iter().map(|item| (None, item));
            write_json_container(members, ('[', ']'), indent, level, out);
        }
        Value::Table(entries) => {
            let members = entries.iter().map(|(key, item)| (Some(key.as_str()), item));
            write_json_container(members, ('{', '}'), indent, level, out);
        }
    }
}

/// One `[...]` or `{...}`, indented per `write_json`'s own rule, or the empty inline form
/// when there are no members at all. `brackets` is `(open, close)`.
fn write_json_container<'a>(
    members: impl Iterator<Item = (Option<&'a str>, &'a Value)>,
    brackets: (char, char),
    indent: usize,
    level: usize,
    out: &mut String,
) {
    let (open, close) = brackets;
    let mut members = members.peekable();
    if members.peek().is_none() {
        out.push(open);
        out.push(close);
        return;
    }
    out.push(open);
    out.push('\n');
    let pad = " ".repeat(indent * (level + 1));
    let mut first = true;
    for (key, item) in members {
        if !first {
            out.push_str(",\n");
        }
        first = false;
        out.push_str(&pad);
        if let Some(key) = key {
            out.push_str(&json_string(key));
            out.push_str(": ");
        }
        write_json(item, indent, level + 1, out);
    }
    out.push('\n');
    out.push_str(&" ".repeat(indent * level));
    out.push(close);
}

/// The whole document, in the layout `dump_toml()` produces.
///
/// A blank line before every section but the first, one `[name]` header, one
/// `key = value` line per scalar, and a single trailing newline -- the Python's
/// `"\n".join(lines) + "\n"`, which is why an empty document comes out as one
/// newline rather than as nothing. Containers sitting where a scalar belongs
/// are dropped without a word, exactly as the Python drops them; a nested table
/// cannot be written by this emitter, so writing one out half-formed would be
/// worse than leaving it out.
///
/// # Errors
///
/// Whatever [`toml_value`] refuses -- in practice only a non-finite float,
/// since the containers it would refuse are skipped before they reach it.
pub fn dump_toml(document: &Document) -> Result<String, EmitError> {
    let mut lines: Vec<String> = Vec::new();
    for (section, values) in document.sections() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("[{section}]"));
        for (key, value) in values.entries() {
            if value.is_container() {
                continue;
            }
            lines.push(format!("{key} = {}", toml_value(value)?));
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(out)
}
