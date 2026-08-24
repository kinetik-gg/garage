//! `CACHE_DIR`, `atomic_write_json()`, `load_usage()`, `load_today()`.
//!
//! Two files, two different staleness policies -- see each function's docs for why. Both
//! read and write through `serde_json` directly: unlike the two payloads
//! [`garage_core::pyjson`] formats for stdout, nothing outside this process ever looks at these
//! bytes, so there is no reason to hold them to Python's `ensure_ascii` formatting -- only
//! to round-trip faithfully, which plain `serde_json` already does (with the
//! `preserve_order` feature, so a passed-through `subscriptions` array keeps tokscale's own
//! key order across a cache round trip).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::tokscale;

/// `TODAY_CACHE_TTL`: how fresh `today.json` has to be before `load_today()` will serve it
/// without re-running tokscale. The scan it stands in for walks session directories and is
/// comparatively expensive -- see `load_today`'s own docs.
// `from_secs(120)`, not `from_mins(2)`, so this reads the same as the Python's
// `TODAY_CACHE_TTL = 120` at a glance.
#[allow(clippy::duration_suboptimal_units)]
pub(crate) const TODAY_CACHE_TTL: Duration = Duration::from_secs(120);

/// `CACHE_DIR` and the two files under it.
#[derive(Debug, Clone)]
pub(crate) struct CachePaths {
    directory: PathBuf,
    usage_file: PathBuf,
    today_file: PathBuf,
}

impl CachePaths {
    pub(crate) fn new(directory: PathBuf) -> Self {
        let usage_file = directory.join("usage.json");
        let today_file = directory.join("today.json");
        Self {
            directory,
            usage_file,
            today_file,
        }
    }
}

/// `atomic_write_json()`: write `value` to `path` so a reader never sees a partial file.
/// `false` on any failure -- there is no error type here because every caller treats a
/// failed write exactly like the read-side failures next to it (see [`load_usage`] and
/// [`load_today`]), never differently.
fn atomic_write_json(path: &Path, value: &Value) -> bool {
    let Ok(text) = serde_json::to_string(value) else {
        return false;
    };
    garage_core::fs::atomic::atomic_write(path, &text).is_ok()
}

/// Read and parse `path` as JSON, folding a missing file, an unreadable one, and invalid
/// JSON into one `None` -- the same three failures the Python's
/// `except (OSError, json.JSONDecodeError)` folds together.
fn read_json_file(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// `read_json_file`, but only if `path`'s mtime is within `ttl` of now. A file that cannot
/// be stat'd, one that is too old, and one that fails to parse all come back `None` alike;
/// the caller (`load_today`) does not distinguish "stale" from "unreadable" either.
fn read_json_file_if_fresh(path: &Path, ttl: Duration) -> Option<Value> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;
    if age >= ttl {
        return None;
    }
    read_json_file(path)
}

/// `valid(payload)`: a JSON array containing at least one object whose `"provider"` is
/// `"Codex"` or `"Claude"`.
pub(crate) fn valid(payload: &Value) -> bool {
    payload.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item.get("provider")
                .and_then(Value::as_str)
                .is_some_and(|name| name == "Codex" || name == "Claude")
        })
    })
}

/// `load_usage(tokscale)`: `(payload, stale)`.
///
/// Tries a fresh `tokscale usage --json` first; if that runs, parses and validates, it is
/// cached to `usage.json` and returned as fresh (`stale = false`). Anything short of that --
/// the subprocess failing, its output not being valid JSON, the JSON not passing
/// [`valid`], or even the cache write itself failing -- falls through to `usage.json` as it
/// last stood, returned as `stale = true` if it validates. Falling all the way through
/// (nothing ran, no cache, or a cache that fails to validate) is `(empty array, stale =
/// true)`, never an error: this is the one function standing between "no usage data
/// reachable at all" and every caller downstream, and none of them are prepared to handle
/// anything but a list.
///
/// That "even the cache write failing falls through" detail is a literal reading of the
/// Python: `atomic_write_json(CACHE_FILE, payload)` sits inside the same `try` as the
/// subprocess call, so an `OSError` out of the write (a full disk, say) is caught by the
/// same `except` that catches a failed subprocess -- the freshly fetched, already-valid
/// payload in hand is discarded in favour of falling through to the cache file, even though
/// nothing has told the cache file itself anything new. Ported here as-is: [`atomic_write_json`]
/// returning `false` is treated the same as the subprocess never having answered.
pub(crate) fn load_usage(tokscale: &Path, paths: &CachePaths) -> (Value, bool) {
    let _ = std::fs::create_dir_all(&paths.directory);

    if let Some(payload) = tokscale::run_tokscale_json(tokscale, &["usage", "--json"]) {
        if valid(&payload) && atomic_write_json(&paths.usage_file, &payload) {
            return (payload, false);
        }
    }

    if let Some(payload) = read_json_file(&paths.usage_file) {
        if valid(&payload) {
            return (payload, true);
        }
    }

    (Value::Array(Vec::new()), true)
}

/// `load_today(tokscale)`.
///
/// Served from `today.json` without touching tokscale at all when that file is younger
/// than [`TODAY_CACHE_TTL`] -- the scan behind `--today` walks every session directory and
/// is comparatively expensive, unlike the weekly-usage summary `load_usage` fetches.
/// Otherwise runs `tokscale --json --today --no-spinner`, caches whatever JSON comes back
/// (no [`valid`] check here -- today's payload has a different, un-validated shape) and
/// returns it; a failed write falls through to the cache file exactly as in [`load_usage`],
/// and reading that has no TTL of its own -- any age is served rather than nothing.
/// `None` only when every one of those has failed, including the final unconditional read.
pub(crate) fn load_today(tokscale: &Path, paths: &CachePaths) -> Option<Value> {
    if let Some(payload) = read_json_file_if_fresh(&paths.today_file, TODAY_CACHE_TTL) {
        return Some(payload);
    }

    if let Some(payload) =
        tokscale::run_tokscale_json(tokscale, &["--json", "--today", "--no-spinner"])
    {
        if atomic_write_json(&paths.today_file, &payload) {
            return Some(payload);
        }
    }

    read_json_file(&paths.today_file)
}

#[cfg(test)]
mod tests {
    use super::{load_today, load_usage, valid, CachePaths, TODAY_CACHE_TTL};
    use std::fs;
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    fn scratch(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "garage-ai-usage-cache-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&path).expect("scratch directory is creatable");
        path
    }

    fn write_shell_script(path: &Path, body: &str) {
        let mut file = fs::File::create(path).expect("script is creatable");
        file.write_all(format!("#!/bin/sh\n{body}\n").as_bytes())
            .expect("script is writable");
        let mut permissions = fs::metadata(path).expect("script metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("script is chmod-able");
    }

    #[test]
    fn valid_requires_a_codex_or_claude_provider() {
        assert!(valid(&serde_json::json!([{"provider": "Codex"}])));
        assert!(valid(&serde_json::json!([{"provider": "Claude"}])));
        assert!(!valid(&serde_json::json!([{"provider": "Other"}])));
        assert!(!valid(&serde_json::json!([])));
        assert!(!valid(&serde_json::json!({"provider": "Codex"})));
        assert!(!valid(&serde_json::json!(null)));
    }

    #[test]
    fn load_usage_caches_a_fresh_valid_payload() {
        let dir = scratch("usage-fresh");
        let script = dir.join("tokscale");
        write_shell_script(&script, r#"echo '[{"provider": "Codex", "plan": "pro"}]'"#);
        let paths = CachePaths::new(dir.join("cache"));

        let (payload, stale) = load_usage(&script, &paths);
        assert!(!stale);
        assert_eq!(
            payload,
            serde_json::json!([{"provider": "Codex", "plan": "pro"}])
        );
        assert!(dir.join("cache").join("usage.json").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_usage_falls_back_to_cache_when_the_subprocess_fails() {
        let dir = scratch("usage-fallback");
        let cache_dir = dir.join("cache");
        fs::create_dir_all(&cache_dir).expect("cache dir");
        fs::write(
            cache_dir.join("usage.json"),
            r#"[{"provider": "Claude", "plan": "max"}]"#,
        )
        .expect("seed cache");
        let missing_script = dir.join("does-not-exist");
        let paths = CachePaths::new(cache_dir);

        let (payload, stale) = load_usage(&missing_script, &paths);
        assert!(stale);
        assert_eq!(
            payload,
            serde_json::json!([{"provider": "Claude", "plan": "max"}])
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_usage_is_an_empty_stale_list_with_nothing_available() {
        let dir = scratch("usage-nothing");
        let missing_script = dir.join("does-not-exist");
        let paths = CachePaths::new(dir.join("cache"));

        let (payload, stale) = load_usage(&missing_script, &paths);
        assert!(stale);
        assert_eq!(payload, serde_json::json!([]));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_usage_ignores_a_subprocess_result_that_does_not_validate() {
        let dir = scratch("usage-invalid");
        let script = dir.join("tokscale");
        write_shell_script(&script, r#"echo '[{"provider": "SomethingElse"}]'"#);
        let paths = CachePaths::new(dir.join("cache"));

        let (payload, stale) = load_usage(&script, &paths);
        assert!(stale);
        assert_eq!(payload, serde_json::json!([]));
        assert!(
            !dir.join("cache").join("usage.json").exists(),
            "an invalid payload must never be cached"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_today_within_ttl_never_touches_tokscale() {
        let dir = scratch("today-fresh-cache");
        let cache_dir = dir.join("cache");
        fs::create_dir_all(&cache_dir).expect("cache dir");
        fs::write(cache_dir.join("today.json"), r#"{"total_cost": 1.5}"#).expect("seed cache");

        // A shim that would prove it ran, by appending to a marker file -- if the cache is
        // honoured this file is never created.
        let marker = dir.join("ran");
        let script = dir.join("tokscale");
        write_shell_script(
            &script,
            &format!("echo ran >> {}\necho '{{}}'", marker.display()),
        );
        let paths = CachePaths::new(cache_dir);

        let payload = load_today(&script, &paths);
        assert_eq!(payload, Some(serde_json::json!({"total_cost": 1.5})));
        assert!(
            !marker.exists(),
            "a fresh cache must short-circuit the subprocess call"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_today_past_ttl_re_runs_tokscale() {
        let dir = scratch("today-stale-cache");
        let cache_dir = dir.join("cache");
        fs::create_dir_all(&cache_dir).expect("cache dir");
        let today_file = cache_dir.join("today.json");
        fs::write(&today_file, r#"{"total_cost": 1.5}"#).expect("seed cache");
        // Back-date the cache file past the TTL.
        let old = SystemTime::now() - TODAY_CACHE_TTL - Duration::from_secs(5);
        set_mtime(&today_file, old);

        let script = dir.join("tokscale");
        write_shell_script(&script, r#"echo '{"total_cost": 9.0}'"#);
        let paths = CachePaths::new(cache_dir);

        let payload = load_today(&script, &paths);
        assert_eq!(payload, Some(serde_json::json!({"total_cost": 9.0})));
        let _ = fs::remove_dir_all(&dir);
    }

    fn set_mtime(path: &Path, when: SystemTime) {
        let file = fs::File::options()
            .write(true)
            .open(path)
            .expect("file opens for mtime update");
        file.set_modified(when).expect("mtime is settable");
    }
}
