//! `_theme_fg()`/`THEME_FG`: the microphone glyph's idle colour, read from the bar's
//! own generated foreground marker so the glyph agrees with the rest of the bar
//! instead of guessing a colour from the theme.

use garage_core::paths::Paths;

/// The fallback the Python hardcodes when the marker file cannot be read.
const FALLBACK: &str = "#f5f5f7";

/// `_theme_fg()`: `(Path.home() / ".local/state/garage/generated/bar-foreground")`.
///
/// Deliberately built from `paths.home` directly rather than
/// `paths.markers.bar_foreground` (which is the same path in the common case, but
/// resolved through `$XDG_STATE_HOME` when that is set to something other than
/// `~/.local/state`). Confirmed live, via the parity script, that the two disagree
/// on a machine with `XDG_STATE_HOME` set: the Python's `_theme_fg()` is written
/// against `Path.home()` and never consults `XDG_STATE_HOME` at all, so this stays
/// literal to that rather than reusing the "more correct" `Paths` resolution the
/// rest of this crate's Garage state reads use.
#[must_use]
pub(crate) fn foreground(paths: &Paths) -> String {
    let marker = paths
        .home
        .join(".local/state/garage/generated/bar-foreground");
    std::fs::read_to_string(marker)
        .map_or_else(|_| FALLBACK.to_string(), |text| text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::foreground;
    use garage_core::paths::Paths;
    use std::collections::HashMap;

    fn paths_under(home: &std::path::Path) -> Paths {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), home.to_string_lossy().into_owned());
        Paths::from_env_map(&env)
    }

    #[test]
    fn falls_back_to_the_hardcoded_colour_when_the_marker_is_missing() {
        let dir = std::env::temp_dir().join(format!(
            "garage-waybar-theme-missing-{}",
            std::process::id()
        ));
        let paths = paths_under(&dir);
        assert_eq!("#f5f5f7", foreground(&paths));
    }

    #[test]
    fn reads_and_trims_the_marker_file_when_present() {
        let dir = std::env::temp_dir().join(format!(
            "garage-waybar-theme-present-{}",
            std::process::id()
        ));
        let marker = dir.join(".local/state/garage/generated/bar-foreground");
        std::fs::create_dir_all(marker.parent().expect("has a parent"))
            .expect("scratch dir is creatable");
        std::fs::write(&marker, "#123456\n").expect("marker is writable");
        let paths = paths_under(&dir);
        assert_eq!("#123456", foreground(&paths));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_xdg_state_home_exactly_as_the_python_does() {
        // The Python's `_theme_fg()` is `Path.home() / ".local/state/..."`, full
        // stop -- it never reads $XDG_STATE_HOME. A Paths resolution that honoured
        // XDG_STATE_HOME here would silently read a different file than the Python
        // on any machine where that variable is set to something else, which is
        // exactly the live divergence the parity script caught.
        let dir =
            std::env::temp_dir().join(format!("garage-waybar-theme-xdg-{}", std::process::id()));
        let elsewhere = std::env::temp_dir().join(format!(
            "garage-waybar-theme-xdg-elsewhere-{}",
            std::process::id()
        ));
        let marker = dir.join(".local/state/garage/generated/bar-foreground");
        std::fs::create_dir_all(marker.parent().expect("has a parent"))
            .expect("scratch dir is creatable");
        std::fs::write(&marker, "#abcdef").expect("marker is writable");

        let mut env = HashMap::new();
        env.insert("HOME".to_string(), dir.to_string_lossy().into_owned());
        env.insert(
            "XDG_STATE_HOME".to_string(),
            elsewhere.to_string_lossy().into_owned(),
        );
        let paths = Paths::from_env_map(&env);

        assert_eq!("#abcdef", foreground(&paths));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
