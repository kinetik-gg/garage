//! `save_primary()` and `activate()`: the sticky "last primary player" file at
//! `~/.cache/waybar-media-player`, and the on-click handler that reads it back to
//! focus whichever window is actually playing.

use std::path::PathBuf;
use std::time::Duration;

use garage_core::paths::Paths;
use serde_json::Value;

use crate::exec::RunError;
use crate::media::browser::BrowserTitleCache;
use crate::media::hyprctl;
use crate::media::render::render;
use crate::media::MediaPayload;

/// Either half of what the Python's outer `try/except (OSError,
/// subprocess.SubprocessError)` catches, unified so `?` can cross both a filesystem
/// call and a `hyprctl` call in the same function.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Failure {
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    Run(RunError),
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<RunError> for Failure {
    fn from(error: RunError) -> Self {
        Self::Run(error)
    }
}

fn state_file(paths: &Paths) -> PathBuf {
    paths.home.join(".cache/waybar-media-player")
}

/// `save_primary(current)`: `if not current["player"]: return`, else create the
/// `.cache` parent and write the player id -- unconditionally, no atomic rename,
/// exactly as `Path.write_text` does.
pub(crate) fn save_primary(paths: &Paths, current: &MediaPayload) -> Result<(), Failure> {
    if current.player.is_empty() {
        return Ok(());
    }
    let path = state_file(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &current.player)?;
    Ok(())
}

/// `activate()`. Reads the sticky player id (falling back to a fresh `render()` if
/// the state file cannot be read), then asks Hyprland to focus whichever open window
/// looks like that player's application.
pub(crate) fn activate(paths: &Paths, browser_cache: &BrowserTitleCache) -> Result<(), Failure> {
    let player = read_sticky_player(paths, browser_cache)?;
    if player.is_empty() {
        return Ok(());
    }
    let player_lower = player.to_lowercase();
    let applications = activation_targets(&player_lower);
    let Some(clients) = hyprctl::clients(Duration::from_secs(2))? else {
        return Ok(());
    };
    focus_matching_window(&clients, &applications)?;
    Ok(())
}

/// `try: player = STATE_FILE.read_text().strip() except OSError: player =
/// render()["player"]`.
fn read_sticky_player(paths: &Paths, browser_cache: &BrowserTitleCache) -> Result<String, Failure> {
    match std::fs::read_to_string(state_file(paths)) {
        Ok(text) => Ok(text.trim().to_string()),
        Err(_) => Ok(render("", browser_cache)?.player),
    }
}

/// The if/elif chain choosing which running applications count as "this player",
/// keyed off substrings of the casefolded MPRIS bus name.
fn activation_targets(player_lower: &str) -> Vec<&str> {
    if player_lower.contains("spotify") {
        return vec!["spotify"];
    }
    if ["chromium", "chrome"]
        .iter()
        .any(|marker| player_lower.contains(marker))
    {
        return vec!["google-chrome", "chromium", "brave", "vivaldi"];
    }
    if player_lower.contains("firefox") {
        return vec!["firefox", "zen"];
    }
    if player_lower.contains("vlc") {
        return vec!["vlc"];
    }
    if player_lower.contains("mpv") {
        return vec!["mpv"];
    }
    vec![player_lower.split('.').next().unwrap_or(player_lower)]
}

/// `for client in reversed(clients): ... if any(...): ... return` -- most-recently
/// listed window first, stopping at (and dispatching a focus for) the first match.
fn focus_matching_window(clients: &[Value], applications: &[&str]) -> Result<(), Failure> {
    for client in clients.iter().rev() {
        let identity =
            hyprctl::client_identity(client, &["class", "initialClass", "title", "initialTitle"]);
        if applications
            .iter()
            .any(|application| identity.contains(application))
        {
            if let Some(address) = hyprctl::client_address(client) {
                dispatch_focus(address)?;
            }
            return Ok(());
        }
    }
    Ok(())
}

/// `crate::exec::run_inherited`, not [`crate::exec::run`]: this is the one
/// `subprocess.run` call in either Python script that does not pass
/// `capture_output=True`, so `hyprctl dispatch`'s own reply is meant to reach
/// whatever launched `--activate`, not be swallowed. See `run_inherited`'s docs.
fn dispatch_focus(address: &str) -> Result<(), RunError> {
    let dispatch = format!("hl.dsp.focus({{ window = \"address:{address}\" }})");
    crate::exec::run_inherited(
        &["/usr/bin/hyprctl", "dispatch", &dispatch],
        Duration::from_secs(2),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{activation_targets, save_primary, state_file};
    use crate::media::MediaPayload;
    use garage_core::paths::Paths;
    use std::collections::HashMap;

    fn paths_under(home: &std::path::Path) -> Paths {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), home.to_string_lossy().into_owned());
        Paths::from_env_map(&env)
    }

    #[test]
    fn spotify_identity_maps_to_the_spotify_application() {
        assert_eq!(
            vec!["spotify"],
            activation_targets("org.mpris.mediaplayer2.spotify")
        );
    }

    #[test]
    fn a_chromium_family_identity_maps_to_every_chromium_browser() {
        assert_eq!(
            vec!["google-chrome", "chromium", "brave", "vivaldi"],
            activation_targets("chromium.instance123")
        );
    }

    #[test]
    fn firefox_maps_to_firefox_and_zen() {
        assert_eq!(
            vec!["firefox", "zen"],
            activation_targets("org.mpris.mediaplayer2.firefox")
        );
    }

    #[test]
    fn an_unrecognised_player_falls_back_to_its_own_first_dotted_segment() {
        assert_eq!(vec!["mpv"], activation_targets("mpv.instance456"));
    }

    #[test]
    fn save_primary_writes_the_sticky_cache_file_the_activate_path_reads_back() {
        let dir = std::env::temp_dir().join(format!("garage-waybar-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir is creatable");
        let paths = paths_under(&dir);
        let current = MediaPayload {
            text: String::new(),
            tooltip: String::new(),
            class: "spotify".to_string(),
            player: "org.mpris.MediaPlayer2.spotify".to_string(),
        };
        save_primary(&paths, &current).expect("writing the sticky cache succeeds");
        let saved = std::fs::read_to_string(state_file(&paths)).expect("the cache file now exists");
        assert_eq!("org.mpris.MediaPlayer2.spotify", saved);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_primary_is_a_no_op_when_there_is_no_primary_player() {
        let dir =
            std::env::temp_dir().join(format!("garage-waybar-test-empty-{}", std::process::id()));
        let paths = paths_under(&dir);
        let current = MediaPayload {
            text: String::new(),
            tooltip: String::new(),
            class: "idle".to_string(),
            player: String::new(),
        };
        save_primary(&paths, &current).expect("a no-op save never errors");
        assert!(!state_file(&paths).exists());
    }
}
