//! The SMB half: which of the shares `ensure-smb-mounted` knows about are mounted.
//!
//! The expected set is every quoted `smb://.../` literal in the helper script, found by
//! treating the file as alternating outside/inside-quotes segments (`str::split('"')`
//! puts every odd-indexed segment inside a pair of quotes). A helper that cannot be read
//! means the machine manages no shares through Garage: the section reports [`Smb::none`]
//! and the chip hides, exactly as the waybar module's idle payload did.

use std::collections::BTreeSet;
use std::time::Duration;

use garage_core::paths::Paths;
use garage_core::process::{run, RunError};

const TIMEOUT: Duration = Duration::from_secs(3);
const HELPER_RELATIVE: &str = ".local/libexec/ensure-smb-mounted";

/// The share state one tick produced. An unreadable helper is `available: false`, not an
/// error: it is the normal state of a machine with no managed shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Smb {
    pub(crate) available: bool,
    pub(crate) expected: usize,
    pub(crate) connected: usize,
    /// Shares with no mount entry, as their last path segment -- what the tooltip names.
    pub(crate) missing_labels: Vec<String>,
}

impl Smb {
    pub(crate) fn none() -> Self {
        Self {
            available: false,
            expected: 0,
            connected: 0,
            missing_labels: Vec::new(),
        }
    }
}

pub(crate) fn probe(paths: &Paths) -> Result<Smb, RunError> {
    let helper = paths.home.join(HELPER_RELATIVE);
    let Ok(source) = std::fs::read_to_string(&helper) else {
        return Ok(Smb::none());
    };
    let expected = expected_shares(&source);
    if expected.is_empty() {
        // A readable helper with no recognisable literal reports an all-connected zero,
        // not `none()` -- the state is real even when it is empty.
        return Ok(status(&expected, &[]));
    }
    let mounted = run(&["gio", "mount", "-l"], TIMEOUT)?;
    let missing = missing_shares(&expected, &mounted.stdout);
    Ok(status(&expected, &missing))
}

fn status(expected: &BTreeSet<String>, missing: &[String]) -> Smb {
    Smb {
        available: true,
        expected: expected.len(),
        connected: expected.len() - missing.len(),
        missing_labels: missing.iter().map(|share| share_label(share)).collect(),
    }
}

/// Every odd quote-segment that spells a share URI.
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

fn missing_shares(expected: &BTreeSet<String>, mounted: &str) -> Vec<String> {
    expected
        .iter()
        .filter(|share| !mounted.contains(&format!(" -> {share}")))
        .cloned()
        .collect()
}

/// The last path segment of a share, after dropping its trailing slash.
fn share_label(share: &str) -> String {
    let trimmed = share.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_owned()
}

#[cfg(test)]
mod tests {
    use super::{expected_shares, missing_shares, share_label, status, Smb};
    use std::collections::BTreeSet;

    fn set<'a>(items: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
        items.into_iter().map(str::to_string).collect()
    }

    #[test]
    fn extracts_quoted_smb_share_literals_from_shell_source() {
        let source = r#"MOUNTS=("smb://nas.local/media/" "smb://nas.local/backup/" "not-a-share")"#;
        assert_eq!(
            set(["smb://nas.local/media/", "smb://nas.local/backup/"]),
            expected_shares(source)
        );
    }

    #[test]
    fn a_quoted_string_with_a_space_is_not_a_match() {
        assert!(expected_shares(r#""smb://nas.local/has space/""#).is_empty());
    }

    #[test]
    fn missing_shares_are_whichever_do_not_appear_mounted() {
        let expected = set(["smb://nas/a/", "smb://nas/b/"]);
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
    fn all_connected_is_available_with_nothing_missing() {
        let expected = set(["smb://nas/a/"]);
        let state = status(&expected, &[]);
        assert!(state.available);
        assert_eq!((1, 1), (state.expected, state.connected));
        assert!(state.missing_labels.is_empty());
    }

    #[test]
    fn a_missing_share_is_named_by_its_label() {
        let expected = set(["smb://nas/a/", "smb://nas/b/"]);
        let missing = vec!["smb://nas/b/".to_string()];
        let state = status(&expected, &missing);
        assert_eq!((2, 1), (state.expected, state.connected));
        assert_eq!(vec!["b".to_string()], state.missing_labels);
    }

    #[test]
    fn none_hides_the_chip() {
        let state = Smb::none();
        assert!(!state.available);
    }
}
