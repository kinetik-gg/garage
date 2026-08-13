//! `hyprctl clients -j`, shared by browser-window disambiguation
//! ([`crate::media::browser`]) and window activation ([`crate::media::activate`]).
//!
//! Both call sites in the Python (`browser_window_titles()` and `activate()`) run the
//! identical `subprocess.run(["/usr/bin/hyprctl", "clients", "-j"], timeout=2,
//! check=False)` and then handle a non-zero exit or invalid JSON as "nothing usable"
//! rather than an error -- only a spawn failure or the timeout expiring propagates.
//! [`clients`] is that shared shape; each caller decides what "nothing usable" means
//! for it.

use std::time::Duration;

use serde_json::Value;

use crate::exec::{self, RunError};

/// `/usr/bin/hyprctl` -- absolute, exactly as the Python hardcodes it.
const HYPRCTL: &str = "/usr/bin/hyprctl";

/// Fetch and parse `hyprctl clients -j`.
///
/// `Ok(None)` covers both of the Python's non-exceptional failure paths: `if
/// result.returncode: return ...` and `except json.JSONDecodeError: return ...`.
/// `Err` is only a spawn failure or the 2-second timeout expiring, for the caller to
/// propagate exactly as the Python's uncaught `OSError`/`SubprocessError` would.
///
/// A successful parse that is not a JSON array (`clients -j` always answers with one
/// in practice) is folded into `Ok(Some(Value::Array(vec![])))` rather than kept as
/// whatever the decoder returned: the Python's `for client in clients` would raise
/// `TypeError` on a non-list here, which is neither of the exceptions the Python's own
/// try/except catches, so it would actually crash the script. Reproducing a crash
/// from a JSON *shape* mismatch (as opposed to invalid JSON, which is handled above)
/// is not worth the fidelity given `hyprctl` never emits one; this is documented as a
/// deliberate, reported deviation.
pub(crate) fn clients(timeout: Duration) -> Result<Option<Vec<Value>>, RunError> {
    let output = exec::run(&[HYPRCTL, "clients", "-j"], timeout)?;
    if output.status != 0 {
        return Ok(None);
    }
    let Ok(parsed) = serde_json::from_str::<Value>(&output.stdout) else {
        return Ok(None);
    };
    Ok(Some(parsed.as_array().cloned().unwrap_or_default()))
}

/// `" ".join(str(client.get(field, "")).casefold() for field in fields)`.
///
/// Only string-valued fields are read as their string; anything else JSON can hold
/// (`hyprctl` never emits `class`/`initialClass`/`title`/`initialTitle`/`address` as
/// non-strings, so this narrowing does not change real-world output) falls back to
/// `""` rather than reproducing Python's `str()` on numbers/booleans/`null`/nested
/// structures -- another small, reported simplification.
#[must_use]
pub(crate) fn client_identity(client: &Value, fields: &[&str]) -> String {
    fields
        .iter()
        .map(|field| {
            client
                .get(field)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `client.get("address")`, as a plain string when present and non-null.
#[must_use]
pub(crate) fn client_address(client: &Value) -> Option<&str> {
    client.get("address").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::client_identity;
    use serde_json::json;

    #[test]
    fn identity_joins_the_requested_fields_lowercased() {
        let client = json!({"class": "Firefox", "initialClass": "firefox", "title": "YouTube"});
        assert_eq!(
            "firefox firefox youtube",
            client_identity(&client, &["class", "initialClass", "title"])
        );
    }

    #[test]
    fn a_missing_field_reads_as_an_empty_string() {
        let client = json!({"class": "Firefox"});
        assert_eq!("firefox ", client_identity(&client, &["class", "title"]));
    }
}
