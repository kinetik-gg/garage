//! Durable metrics-history location, resolved once from the environment.

use std::collections::HashMap;
use std::path::PathBuf;

/// Every location the collector writes.
#[derive(Debug, Clone)]
pub(crate) struct Dirs {
    /// `$XDG_STATE_HOME/garage/metrics/history.json`.
    pub(crate) history: PathBuf,
}

impl Dirs {
    /// Resolve the state file from the real process environment.
    pub(crate) fn from_env() -> Self {
        let environment: HashMap<String, String> = std::env::vars().collect();
        Self::from_map(&environment)
    }

    fn from_map(environment: &HashMap<String, String>) -> Self {
        let home = PathBuf::from(environment.get("HOME").map_or("/", String::as_str));
        let state_home = xdg(environment, "XDG_STATE_HOME", home.join(".local/state"));
        Self {
            history: state_home.join("garage/metrics/history.json"),
        }
    }
}

fn xdg(environment: &HashMap<String, String>, name: &str, fallback: PathBuf) -> PathBuf {
    let value = environment.get(name).map_or("", String::as_str).trim();
    if value.starts_with('/') {
        return PathBuf::from(value);
    }
    fallback
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::Dirs;

    fn resolve(pairs: &[(&str, &str)]) -> Dirs {
        let environment = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<HashMap<_, _>>();
        Dirs::from_map(&environment)
    }

    #[test]
    fn default_and_xdg_locations_name_one_history_file() {
        assert_eq!(
            resolve(&[("HOME", "/home/tester")]).history,
            PathBuf::from("/home/tester/.local/state/garage/metrics/history.json")
        );
        assert_eq!(
            resolve(&[("HOME", "/home/tester"), ("XDG_STATE_HOME", "/run/state")]).history,
            PathBuf::from("/run/state/garage/metrics/history.json")
        );
    }

    #[test]
    fn empty_and_relative_xdg_values_fall_back() {
        for value in ["", "relative", "~/state"] {
            assert_eq!(
                resolve(&[("HOME", "/home/tester"), ("XDG_STATE_HOME", value)]).history,
                PathBuf::from("/home/tester/.local/state/garage/metrics/history.json")
            );
        }
    }
}
