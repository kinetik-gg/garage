//! Where the history, the rendered strips and the bar's colour live.
//!
//! The Python resolves all of this at import: `HOME`, two XDG base directories,
//! `STATE_DIR`, `CACHE_DIR` and `THEME_FG` are module-level constants, computed once
//! before any mode runs. [`Dirs`] is that same one-time resolution as a value the
//! process builds at startup and threads through, which is the pattern
//! `garage_core::paths::Paths` already sets in this workspace: no statics, so a test
//! redirects the whole tree by constructing one rather than by mutating the
//! environment out from under whatever else is running.
//!
//! `THEME_FG` travelling in here rather than in a global is the one structural
//! departure from the Python worth naming. It is a module constant there and the render
//! functions reach it directly; here it is a field the renderers are handed. Same value,
//! read the same way at the same moment -- and a render test can now assert against a
//! known colour instead of whichever wallpaper the machine running the suite is on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The colour to fall back to when the bar has not published one -- the same near-white
/// the Python names.
const DEFAULT_FG: &str = "#f5f5f7";

/// The six widgets, in the order `--help` lists them and `seed_object` walks them.
pub(crate) const WIDGETS: [&str; 6] = ["cpu", "memory", "network", "temp", "disk", "gpu"];

/// Every location this binary reads or writes, resolved once from the environment.
#[derive(Debug, Clone)]
pub(crate) struct Dirs {
    /// `$XDG_STATE_HOME/garage/metrics` -- one JSON file and one lock file per widget.
    pub(crate) state: PathBuf,
    /// `$XDG_CACHE_HOME/garage-metrics` -- one rendered SVG per widget.
    pub(crate) cache: PathBuf,
    /// The colour the bar picked from the wallpaper.
    pub(crate) foreground: String,
}

impl Dirs {
    /// Resolve from the real process environment, as the Python's import does.
    pub(crate) fn from_env() -> Self {
        let environment: HashMap<String, String> = std::env::vars().collect();
        Self::from_map(&environment, &read_foreground)
    }

    /// The resolution itself, over a map and an injected reader for the colour file.
    ///
    /// History lives under state and the rendered SVG under cache on purpose: the
    /// history is the thing whose loss is visible (the graph goes flat), the SVG is
    /// regenerated on the next tick two seconds later.
    fn from_map(
        environment: &HashMap<String, String>,
        foreground: &dyn Fn(&Path) -> String,
    ) -> Self {
        let home = PathBuf::from(environment.get("HOME").map_or("/", String::as_str));
        let state_home = xdg(environment, "XDG_STATE_HOME", home.join(".local/state"));
        let cache_home = xdg(environment, "XDG_CACHE_HOME", home.join(".cache"));
        Self {
            state: state_home.join("garage").join("metrics"),
            cache: cache_home.join("garage-metrics"),
            foreground: foreground(&state_home.join("garage/generated/bar-foreground")),
        }
    }

    /// The state file holding one widget's rolling history and raw counters.
    pub(crate) fn state_file(&self, widget: &str) -> PathBuf {
        self.state.join(format!("{widget}.json"))
    }

    /// The lock two overlapping ticks contend for.
    pub(crate) fn lock_file(&self, widget: &str) -> PathBuf {
        self.state.join(format!("{widget}.lock"))
    }

    /// The rendered strip Waybar's image module reads.
    pub(crate) fn svg_file(&self, widget: &str) -> PathBuf {
        self.cache.join(format!("{widget}.svg"))
    }
}

/// An XDG base directory, treating an empty value as unset.
///
/// `XDG_STATE_HOME=` in the environment is common -- a shell profile that exports the
/// variable before deciding what to put in it, a systemd unit with an empty
/// `Environment=` line -- and reading it back naively would hand over `""`, which
/// Python's `Path` turns into `.` and every state file lands in whatever directory the
/// bar happened to start from. The spec says an empty value means unset.
///
/// The `startswith("/")` test is the Python's, and it does more than reject the empty
/// string: a relative value is unset too, which is also what the spec says.
fn xdg(environment: &HashMap<String, String>, name: &str, fallback: PathBuf) -> PathBuf {
    let value = environment.get(name).map_or("", String::as_str).trim();
    if value.starts_with('/') {
        return PathBuf::from(value);
    }
    fallback
}

/// The colour the bar picked from the wallpaper.
///
/// These widgets emit their own markup, so the bar's stylesheet cannot reach inside
/// them. Deriving a colour from the theme instead of reading the one the bar published
/// is what made the old graphs disagree with every other module in the bar.
fn read_foreground(path: &Path) -> String {
    std::fs::read_to_string(path)
        .map_or_else(|_| DEFAULT_FG.to_string(), |text| text.trim().to_string())
}

#[cfg(test)]
impl Dirs {
    /// A [`Dirs`] pointing at a scratch tree, for tests that write files.
    pub(crate) fn scratch(root: &Path) -> Self {
        Self {
            state: root.join("state"),
            cache: root.join("cache"),
            foreground: DEFAULT_FG.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Dirs, DEFAULT_FG};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    fn resolve(pairs: &[(&str, &str)]) -> Dirs {
        Dirs::from_map(&env(pairs), &|_| DEFAULT_FG.to_string())
    }

    #[test]
    fn the_defaults_hang_off_home() {
        let dirs = resolve(&[("HOME", "/home/tester")]);
        assert_eq!(
            dirs.state,
            PathBuf::from("/home/tester/.local/state/garage/metrics")
        );
        assert_eq!(
            dirs.cache,
            PathBuf::from("/home/tester/.cache/garage-metrics")
        );
    }

    #[test]
    fn an_absolute_xdg_value_wins() {
        let dirs = resolve(&[
            ("HOME", "/home/tester"),
            ("XDG_STATE_HOME", "/run/state"),
            ("XDG_CACHE_HOME", "/run/cache"),
        ]);
        assert_eq!(dirs.state, PathBuf::from("/run/state/garage/metrics"));
        assert_eq!(dirs.cache, PathBuf::from("/run/cache/garage-metrics"));
    }

    #[test]
    fn an_empty_or_relative_xdg_value_is_unset() {
        for value in ["", "   ", "relative/path", "~/state"] {
            let dirs = resolve(&[("HOME", "/home/tester"), ("XDG_STATE_HOME", value)]);
            assert_eq!(
                dirs.state,
                PathBuf::from("/home/tester/.local/state/garage/metrics"),
                "XDG_STATE_HOME={value:?} should have fallen back"
            );
        }
    }

    #[test]
    fn the_foreground_is_read_from_under_the_state_home() {
        let seen = std::cell::RefCell::new(PathBuf::new());
        let dirs = Dirs::from_map(&env(&[("HOME", "/home/tester")]), &|path: &Path| {
            *seen.borrow_mut() = path.to_path_buf();
            "#ff0000".to_string()
        });
        assert_eq!(
            *seen.borrow(),
            PathBuf::from("/home/tester/.local/state/garage/generated/bar-foreground")
        );
        assert_eq!(dirs.foreground, "#ff0000");
    }

    #[test]
    fn the_per_widget_paths_are_named_after_the_widget() {
        let dirs = resolve(&[("HOME", "/home/tester")]);
        assert!(dirs.state_file("cpu").ends_with("garage/metrics/cpu.json"));
        assert!(dirs.lock_file("gpu").ends_with("garage/metrics/gpu.lock"));
        assert!(dirs.svg_file("disk").ends_with("garage-metrics/disk.svg"));
    }
}
