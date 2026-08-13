//! `smb()`: which of the shares `ensure-smb-mounted` knows about are actually
//! mounted right now.
//!
//! `re.findall(r'"(smb://[^" ]+/)"', helper.read_text())` pulls every quoted
//! `smb://.../` literal out of the helper script. [`expected_shares`] reproduces that
//! by treating the file as alternating outside/inside-quotes segments (`str::split('"')`
//! puts every odd-indexed segment inside a pair of quotes, which is exactly what the
//! regex's own quote-bounded match requires) rather than adding a `regex` dependency
//! for one pattern.

use std::collections::BTreeSet;
use std::time::Duration;

use crate::context::run::run_ignoring_status;
use crate::exec::RunError;
use crate::waybar::Payload;
use garage_core::paths::Paths;

const TIMEOUT: Duration = Duration::from_secs(3);
const HELPER_RELATIVE: &str = ".local/libexec/ensure-smb-mounted";

pub(crate) fn payload(paths: &Paths) -> Result<Payload, RunError> {
    let helper = paths.home.join(HELPER_RELATIVE);
    let Ok(source) = std::fs::read_to_string(&helper) else {
        return Ok(Payload::idle());
    };
    let expected = expected_shares(&source);
    if expected.is_empty() {
        // Ported as-is: an empty `expected` set falls into the Python's `else`
        // branch below (`missing` is also empty), not the `if not helper.exists()`
        // early return -- a helper file with no recognisable share literal reports
        // "SMB 0 / All 0 SMB shares connected", not idle.
        return Ok(status_payload(&expected, &[]));
    }
    let mounted = run_ignoring_status(&["gio", "mount", "-l"], TIMEOUT)?;
    let missing = missing_shares(&expected, &mounted);
    Ok(status_payload(&expected, &missing))
}

/// `set(re.findall(r'"(smb://[^" ]+/)"', text))`.
fn expected_shares(source: &str) -> BTreeSet<String> {
    source
        .split('"')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, content)| content)
        .filter(|content| is_share_literal(content))
        .map(str::to_string)
        .collect()
}

fn is_share_literal(content: &str) -> bool {
    content
        .strip_prefix("smb://")
        .is_some_and(|rest| !rest.is_empty() && rest.ends_with('/') && !rest.contains(' '))
}

/// `sorted(share for share in expected if f" -> {share}" not in mounted)`.
fn missing_shares(expected: &BTreeSet<String>, mounted: &str) -> Vec<String> {
    expected
        .iter()
        .filter(|share| !mounted.contains(&format!(" -> {share}")))
        .cloned()
        .collect()
}

/// The last path segment of a share, after dropping its trailing slash --
/// `share.rstrip("/").rsplit("/", 1)[-1]`.
fn share_label(share: &str) -> &str {
    let trimmed = share.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

fn status_payload(expected: &BTreeSet<String>, missing: &[String]) -> Payload {
    let connected = expected.len() - missing.len();
    if missing.is_empty() {
        Payload {
            text: format!("SMB {connected}"),
            tooltip: format!("All {connected} SMB shares connected"),
            class: "active".to_string(),
        }
    } else {
        let labels: Vec<&str> = missing.iter().map(|share| share_label(share)).collect();
        Payload {
            text: format!("SMB {connected}"),
            tooltip: format!(
                "Connected {connected} / {}\nUnavailable\n{}",
                expected.len(),
                labels.join("\n")
            ),
            class: "warning".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{expected_shares, missing_shares, share_label, status_payload};
    use std::collections::BTreeSet;

    #[test]
    fn extracts_quoted_smb_share_literals_from_shell_source() {
        let source = r#"MOUNTS=("smb://nas.local/media/" "smb://nas.local/backup/" "not-a-share")"#;
        let expected: BTreeSet<String> = ["smb://nas.local/media/", "smb://nas.local/backup/"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(expected, expected_shares(source));
    }

    #[test]
    fn a_quoted_string_with_a_space_is_not_a_match() {
        let source = r#""smb://nas.local/has space/""#;
        assert!(expected_shares(source).is_empty());
    }

    #[test]
    fn missing_shares_are_whichever_do_not_appear_mounted() {
        let expected: BTreeSet<String> = ["smb://nas/a/", "smb://nas/b/"]
            .into_iter()
            .map(str::to_string)
            .collect();
        // Real `gio mount -l` output reads "Mount(0): <label> -> <uri>": the arrow
        // points at the share URL, not away from it.
        let mounted = "Mount(0): smb-share:server=nas,share=a -> smb://nas/a/";
        assert_eq!(
            vec!["smb://nas/b/".to_string()],
            missing_shares(&expected, mounted)
        );
    }

    #[test]
    fn share_label_takes_the_final_path_segment() {
        assert_eq!("media", share_label("smb://nas.local/media/"));
    }

    #[test]
    fn all_connected_reports_the_active_class() {
        let expected: BTreeSet<String> = ["smb://nas/a/"].into_iter().map(str::to_string).collect();
        let payload = status_payload(&expected, &[]);
        assert_eq!("SMB 1", payload.text);
        assert_eq!("All 1 SMB shares connected", payload.tooltip);
        assert_eq!("active", payload.class);
    }

    #[test]
    fn a_missing_share_reports_the_warning_class_and_its_label() {
        let expected: BTreeSet<String> = ["smb://nas/a/", "smb://nas/b/"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let missing = vec!["smb://nas/b/".to_string()];
        let payload = status_payload(&expected, &missing);
        assert_eq!("SMB 1", payload.text);
        assert_eq!("Connected 1 / 2\nUnavailable\nb", payload.tooltip);
        assert_eq!("warning", payload.class);
    }
}
