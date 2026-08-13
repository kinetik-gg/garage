//! Reading the plan back off the fragment that is still installed.
//!
//! `INSTALLED_GROUP` (garage:852-853) is a regular expression in the Python and a hand-rolled
//! scanner here, for the reason its own comment gives: it is anchored on the field names
//! rather than on the line, so it survives the fragment being reformatted, and that is a
//! shape three `starts_with`es express as exactly as a regex engine would. A dependency for
//! one pattern read once per workspace apply would be the more surprising choice.

use crate::cx::SessionCx;

/// The groups the fragment now on disk hands out.
///
/// Read back rather than recomputed, because it is the only record of the plan the compositor
/// is still running -- which is to say, of the ids the windows are currently sitting on.
/// Scraped with a pattern rather than executed: this is generated Lua being read by the
/// process that writes it, and running it would be a second language in the loop for three
/// integers.
///
/// Anything unreadable comes back empty, which makes the remap a no-op. A plan that cannot be
/// compared is one nothing should be moved on.
pub(super) fn installed_workspace_groups(cx: &SessionCx<'_>) -> Vec<(String, u32, u32)> {
    let Ok(text) = std::fs::read_to_string(&cx.render().paths().fragments.workspaces) else {
        return Vec::new();
    };
    if !text.contains("mode = \"per-display\"") {
        // Shared pins nothing, so there is no display-owned id to map from.
        return Vec::new();
    }
    scan_installed_groups(&text)
}

/// `INSTALLED_GROUP.findall(text)` (garage:852-853), hand-rolled.
///
/// The pattern is
/// `monitor\s*=\s*"([^"]*)"\s*,\s*first\s*=\s*(\d+)\s*,\s*count\s*=\s*(\d+)`, anchored on
/// the field names rather than on the line so it survives the fragment being reformatted.
/// `findall` is leftmost and non-overlapping: a match resumes scanning at its own end, and a
/// position that fails to match advances by one. That is exactly the loop below, so a regex
/// crate would buy nothing but a dependency.
pub(crate) fn scan_installed_groups(text: &str) -> Vec<(String, u32, u32)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match match_group(bytes, at) {
            Some((group, end)) => {
                found.push(group);
                at = end;
            }
            None => at += 1,
        }
    }
    found
}

/// One attempt at the pattern starting exactly at `at`, and where it ended.
fn match_group(bytes: &[u8], at: usize) -> Option<((String, u32, u32), usize)> {
    let mut at = literal(bytes, at, b"monitor")?;
    at = spaced(bytes, at, b'=')?;
    at = literal(bytes, at, b"\"")?;
    let start = at;
    while bytes.get(at).is_some_and(|byte| *byte != b'"') {
        at += 1;
    }
    let monitor = String::from_utf8_lossy(bytes.get(start..at)?).into_owned();
    at = literal(bytes, at, b"\"")?;
    at = spaced(bytes, at, b',')?;
    at = literal(bytes, at, b"first")?;
    at = spaced(bytes, at, b'=')?;
    let (first, at) = digits(bytes, at)?;
    let mut at = spaced(bytes, at, b',')?;
    at = literal(bytes, at, b"count")?;
    at = spaced(bytes, at, b'=')?;
    let (count, at) = digits(bytes, at)?;
    Some(((monitor, first, count), at))
}

fn literal(bytes: &[u8], at: usize, word: &[u8]) -> Option<usize> {
    let end = at.checked_add(word.len())?;
    (bytes.get(at..end)? == word).then_some(end)
}

/// `\s*<byte>\s*`. Python's `\s` is ASCII whitespace here, which is all a generated fragment
/// can contain between two fields.
fn spaced(bytes: &[u8], at: usize, wanted: u8) -> Option<usize> {
    let mut at = skip_space(bytes, at);
    if *bytes.get(at)? != wanted {
        return None;
    }
    at += 1;
    Some(skip_space(bytes, at))
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

/// `(\d+)`, saturating rather than failing: a digit run too long for a `u32` is a workspace
/// id no compositor will ever report, and refusing the whole match would silently drop the
/// two neighbouring groups with it.
fn digits(bytes: &[u8], at: usize) -> Option<(u32, usize)> {
    let mut end = at;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == at {
        return None;
    }
    let text = String::from_utf8_lossy(bytes.get(at..end)?).into_owned();
    Some((text.parse::<u32>().unwrap_or(u32::MAX), end))
}
