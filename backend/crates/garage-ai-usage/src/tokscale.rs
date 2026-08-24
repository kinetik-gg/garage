//! `find_tokscale()`, and `run_tokscale_json()`, the subprocess wrapper `load_usage()` and
//! `load_today()` share.
//!
//! Tokscale is optional: this whole module exists to answer "where is it, if anywhere" and
//! to run it the same two ways the Python does, without ever turning its absence into a
//! failure the caller has to handle specially. See the crate root docs. Spawning itself
//! goes through [`garage_core::process::run`], shared by the leaf data helpers.

use std::path::{Path, PathBuf};
use std::time::Duration;

use garage_core::process;

/// The one place the Python looks for tokscale before falling back to `PATH`:
/// `~/.local/share/tokscale/node_modules/.bin/tokscale`. A `Vec` because
/// `TOKSCALE_CANDIDATES` is a list in the Python, even though it only ever holds one entry
/// today -- a caller that wants to add a second search location changes one call site.
pub(crate) fn tokscale_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    match home {
        Some(home) => vec![home.join(".local/share/tokscale/node_modules/.bin/tokscale")],
        // No $HOME to resolve a candidate against -- fall through to a PATH-only search,
        // same as the Python would if `Path.home()` somehow produced nothing usable.
        None => Vec::new(),
    }
}

/// `find_tokscale()`: the first candidate that is an executable file, else the first
/// `tokscale` found on `PATH`, else `None`.
pub(crate) fn find_tokscale(candidates: &[PathBuf], path_env: Option<&str>) -> Option<PathBuf> {
    for candidate in candidates {
        if is_executable_file(candidate) {
            return Some(candidate.clone());
        }
    }
    which("tokscale", path_env)
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

/// `shutil.which(name)`, searching `path_env` (`$PATH`) left to right.
fn which(name: &str, path_env: Option<&str>) -> Option<PathBuf> {
    let path_env = path_env?;
    std::env::split_paths(&path_env)
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable_file(candidate))
}

/// Run `/usr/bin/timeout 5 <tokscale> <args...>`, capture stdout, and parse it as JSON.
///
/// Mirrors the Python's combined try block exactly: a subprocess that cannot be spawned,
/// that does not finish inside the overall timeout, or whose stdout is not valid JSON, all
/// collapse to `None` here the same way they all fall into the Python's
/// `except (OSError, subprocess.SubprocessError, json.JSONDecodeError): pass`. The exit
/// code is never consulted -- `subprocess.run(..., check=False)` never raises on a non-zero
/// exit, and neither `load_usage()` nor `load_today()` reads `result.returncode`.
///
/// The outer ten-second cap is [`garage_core::process::run`]'s `timeout` argument, standing in for
/// the Python's own `subprocess.run(..., timeout=10)` -- a safety net around the inner
/// `/usr/bin/timeout 5`, in case `timeout` itself is what is missing or wedged.
pub(crate) fn run_tokscale_json(tokscale: &Path, args: &[&str]) -> Option<serde_json::Value> {
    let tokscale_str = tokscale.to_str()?;
    let mut command: Vec<&str> = vec!["/usr/bin/timeout", "5", tokscale_str];
    command.extend_from_slice(args);
    let output = process::run(&command, Duration::from_secs(10)).ok()?;
    serde_json::from_str(&output.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::{find_tokscale, is_executable_file, run_tokscale_json, which};
    use std::fs;
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "garage-ai-usage-tokscale-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&path).expect("scratch directory is creatable");
        path
    }

    fn write_shell_script(path: &std::path::Path, body: &str) {
        let mut file = fs::File::create(path).expect("script is creatable");
        file.write_all(format!("#!/bin/sh\n{body}\n").as_bytes())
            .expect("script is writable");
        let mut permissions = fs::metadata(path).expect("script metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("script is chmod-able");
    }

    #[test]
    fn a_non_executable_candidate_is_rejected() {
        let dir = scratch("non-exec");
        let candidate = dir.join("tokscale");
        fs::write(&candidate, "not a script").expect("file is writable");
        assert!(!is_executable_file(&candidate));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_executable_candidate_is_accepted() {
        let dir = scratch("exec");
        let candidate = dir.join("tokscale");
        write_shell_script(&candidate, "exit 0");
        assert!(is_executable_file(&candidate));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_tokscale_prefers_the_candidate_over_path() {
        let dir = scratch("candidate-wins");
        let candidate = dir.join("tokscale");
        write_shell_script(&candidate, "exit 0");
        let found = find_tokscale(std::slice::from_ref(&candidate), None);
        assert_eq!(found, Some(candidate));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_tokscale_falls_back_to_path() {
        let dir = scratch("path-fallback");
        let on_path = dir.join("tokscale");
        write_shell_script(&on_path, "exit 0");
        let missing_candidate = dir.join("does-not-exist").join("tokscale");
        let path_env = dir.to_string_lossy().into_owned();
        let found = find_tokscale(&[missing_candidate], Some(&path_env));
        assert_eq!(found, Some(on_path));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_tokscale_is_none_when_nothing_matches() {
        let dir = scratch("nothing");
        let missing = dir.join("nope").join("tokscale");
        assert_eq!(which("tokscale", None), None);
        assert_eq!(find_tokscale(&[missing], None), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_tokscale_json_parses_stdout_as_json() {
        let dir = scratch("json-ok");
        let script = dir.join("tokscale");
        write_shell_script(&script, r#"echo '[{"provider": "Codex"}]'"#);
        let value = run_tokscale_json(&script, &["usage", "--json"]).expect("valid json");
        assert_eq!(value, serde_json::json!([{"provider": "Codex"}]));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_tokscale_json_is_none_on_invalid_json() {
        let dir = scratch("json-bad");
        let script = dir.join("tokscale");
        write_shell_script(&script, "echo 'not json'");
        assert_eq!(run_tokscale_json(&script, &["usage", "--json"]), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_tokscale_json_is_none_when_the_binary_is_missing() {
        let dir = scratch("missing-binary");
        let script = dir.join("does-not-exist");
        assert_eq!(run_tokscale_json(&script, &["usage", "--json"]), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
