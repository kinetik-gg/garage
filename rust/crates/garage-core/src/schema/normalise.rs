//! `normalise`: the one kind that rewrites its value instead of checking it.
//!
//! The schema banner calls it out as the exception -- "rewrites the value
//! instead of checking it, with no note. For the one preference whose stored
//! form is a parsed structure" -- and the rewrite happens before the kind check
//! rather than instead of it, exactly as the Python's pass does: the entry's
//! `normalise` runs first, and the `"normalised"` kind that follows is the
//! predicate that always passes.

use crate::schema::coerce::{Coerce, PyTruthy, Store};
use crate::schema::newtypes::WORKSPACE_BLOCK;
use crate::schema::notes::py_str_toml;

/// There are only ten number keys to bind, so ten is the ceiling everywhere a
/// workspace count appears. It is also how wide a display's block of ids is,
/// which is why `WORKSPACE_BLOCK` is defined as this same number.
pub const WORKSPACE_COUNT_MAX: u32 = WORKSPACE_BLOCK;

/// `"normalised"`: `workspaces.counts`, rewritten rather than checked.
///
/// The pane sends the whole map on every edit, so rewriting it as whatever
/// parsing understood means a hand-typed stray comma is dropped once instead of
/// being carried back out to the file on every later save.
/// [`parse_workspace_counts`] is lenient by design, so there is no invalid
/// value here to coerce, only a value to restate -- which is why this type's
/// [`Coerce`] never returns `None` and the key is never reported.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct WorkspaceCounts(String);

impl WorkspaceCounts {
    /// The normalised `OUTPUT=N,OUTPUT=N` text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The counts themselves, in the order they were written.
    #[must_use]
    pub fn parsed(&self) -> Vec<(String, u32)> {
        parse_workspace_counts(&self.0)
    }
}

impl Coerce for WorkspaceCounts {
    fn coerce(value: &toml::Value) -> Option<Self> {
        Some(Self(format_workspace_counts(&parse_workspace_counts(
            &py_str_or_empty(value),
        ))))
    }
}

impl Store for WorkspaceCounts {
    fn store(&self) -> toml::Value {
        toml::Value::String(self.0.clone())
    }
}

/// `str(value or "")`, the expression `parse_workspace_counts()` opens with.
fn py_str_or_empty(value: &toml::Value) -> String {
    if PyTruthy::coerce(value).is_some_and(PyTruthy::get) {
        py_str_toml(value)
    } else {
        String::new()
    }
}

/// The per-output counts encoded in `workspaces.counts`.
///
/// Lenient in the same way the locale coercion is: this is the one preference a
/// human is at all likely to hand-edit, and one bad entry must not make the
/// file unloadable and the whole UI read-only with no way to correct it from
/// inside. An entry that does not parse is dropped; a count outside the range
/// is clamped into it rather than dropped, because the output it names is still
/// wanted.
#[must_use]
pub fn parse_workspace_counts(value: &str) -> Vec<(String, u32)> {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for entry in value.split(',') {
        let Some((output, size)) = entry.split_once('=') else {
            continue;
        };
        let (output, size) = (output.trim(), size.trim());
        if output.is_empty() || size.is_empty() || !size.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        // A digit string too long for u64 is still a number far above the
        // ceiling, so it clamps to the ceiling rather than being dropped.
        let count = size.parse::<u64>().unwrap_or(u64::MAX);
        let count = count.clamp(1, u64::from(WORKSPACE_COUNT_MAX));
        let count = u32::try_from(count).unwrap_or(WORKSPACE_COUNT_MAX);
        match counts.iter_mut().find(|(name, _)| name == output) {
            // A repeated output keeps its first position and its last value,
            // which is what assigning into a dict does.
            Some(existing) => existing.1 = count,
            None => counts.push((output.to_string(), count)),
        }
    }
    counts
}

/// The counts back as one scalar. The schema's leaves are deliberately not
/// list-valued: the pane presents them as a list and never asks the user to
/// edit the serialization.
#[must_use]
pub fn format_workspace_counts(counts: &[(String, u32)]) -> String {
    counts
        .iter()
        .map(|(output, size)| format!("{output}={size}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::{format_workspace_counts, parse_workspace_counts, WorkspaceCounts};
    use crate::schema::coerce::Coerce;

    fn parse(text: &str) -> toml::Value {
        let table: toml::Table = format!("value = {text}").parse().unwrap();
        table.get("value").unwrap().clone()
    }

    #[test]
    fn workspace_counts_are_restated_rather_than_refused() {
        let counts = |text: &str| WorkspaceCounts::coerce(&parse(text)).unwrap();
        assert_eq!(counts("\"DP-1=3,DP-2=4\"").as_str(), "DP-1=3,DP-2=4");
        // A stray comma, a missing separator, a blank name and an out-of-range
        // count: dropped, dropped, dropped, clamped.
        assert_eq!(
            counts("\"DP-1=3,,junk, =2,DP-2=99\"").as_str(),
            "DP-1=3,DP-2=10"
        );
        assert_eq!(counts("\" DP-1 = 0 \"").as_str(), "DP-1=1");
        assert_eq!(counts("\"DP-1=2,DP-1=5\"").as_str(), "DP-1=5");
        assert_eq!(counts("false").as_str(), "");
        assert_eq!(counts("[]").as_str(), "");
    }

    #[test]
    fn workspace_counts_round_trip() {
        let parsed = parse_workspace_counts("DP-1=3,HDMI-A-1=2");
        assert_eq!(
            parsed,
            [("DP-1".to_string(), 3), ("HDMI-A-1".to_string(), 2)]
        );
        assert_eq!(format_workspace_counts(&parsed), "DP-1=3,HDMI-A-1=2");
        assert_eq!(format_workspace_counts(&[]), "");
        assert_eq!(
            WorkspaceCounts::coerce(&parse("\"DP-1=3\""))
                .unwrap()
                .parsed(),
            [("DP-1".to_string(), 3)]
        );
    }
}
