//! `playerctl` plumbing: `run()`, `metadata()`, `playing_players()` and
//! `primary_player()` from `media-status.py`, ported field-for-field.

use std::time::Duration;

use crate::exec::{self, RunError};
use crate::media::source;

/// `/usr/bin/playerctl` -- an absolute path in the Python, not resolved off `PATH`.
/// See the crate's parity notes: a `PATH` shim cannot intercept this call, only a
/// real `/usr/bin/playerctl` (or a bind-mount/symlink over it) can.
const PLAYERCTL: &str = "/usr/bin/playerctl";

/// `playerctl`'s 2-second timeout, shared by every call the Python makes through its
/// `run()` helper.
const PLAYERCTL_TIMEOUT: Duration = Duration::from_secs(2);

/// `\x1f`, the field separator `METADATA_FORMAT` joins on. Not a character any real
/// artist/title/album/URL is going to contain.
const SEPARATOR: char = '\u{1f}';

/// `playing_players()`'s per-player record -- the dict `metadata()` returns, made
/// concrete. `player` is the MPRIS bus suffix (`playerctl --list-all`'s own output,
/// e.g. `spotify` or `chromium.instance123`), not a display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlayerInfo {
    pub(crate) player: String,
    pub(crate) status: String,
    pub(crate) artist: String,
    pub(crate) title: String,
    pub(crate) album: String,
    pub(crate) style: &'static str,
    pub(crate) source: String,
    /// The Nerd Font glyph [`crate::media::source::classify`] picked for this
    /// player's source, shown in the icon run built by
    /// [`crate::media::render::render`].
    pub(crate) icon: &'static str,
}

/// `run(*arguments)`: invoke `playerctl` with `arguments`, returning its stdout on a
/// zero exit and `""` on anything else -- a non-zero exit is not this function's
/// business, only a spawn failure or the timeout expiring is (propagated as
/// [`RunError`], for the caller to let bubble all the way to an idle payload exactly
/// as the Python's uncaught `OSError`/`SubprocessError` does).
fn run(arguments: &[&str]) -> Result<String, RunError> {
    let mut argv = Vec::with_capacity(arguments.len() + 1);
    argv.push(PLAYERCTL);
    argv.extend_from_slice(arguments);
    let output = exec::run(&argv, PLAYERCTL_TIMEOUT)?;
    Ok(if output.status == 0 {
        output.stdout
    } else {
        String::new()
    })
}

/// `metadata(player, browser_titles="")`: fetch one player's `METADATA_FORMAT` line
/// and classify it via [`source::classify`]. `None` on an empty answer or a field
/// count that is not exactly 7, matching the Python's two early `return None`s.
pub(crate) fn metadata(player: &str, browser_titles: &str) -> Result<Option<PlayerInfo>, RunError> {
    let format = [
        "{{status}}",
        "{{playerName}}",
        "{{artist}}",
        "{{title}}",
        "{{album}}",
        "{{xesam:url}}",
        "{{mpris:artUrl}}",
    ]
    .join(&SEPARATOR.to_string());
    let output = run(&["--player", player, "metadata", "--format", &format])?;
    if output.is_empty() {
        return Ok(None);
    }
    let trimmed = output.trim_end_matches('\n');
    let fields: Vec<&str> = trimmed.split(SEPARATOR).collect();
    let [status, identity, artist, title, album, url, artwork] = fields.as_slice() else {
        return Ok(None);
    };
    let evidence_player = format!("{player} {identity}");
    let classified = source::classify(&evidence_player, url, artwork, browser_titles);
    Ok(Some(PlayerInfo {
        player: player.to_string(),
        status: status.to_lowercase(),
        artist: (*artist).to_string(),
        title: (*title).to_string(),
        album: (*album).to_string(),
        style: classified.style,
        source: classified.label,
        icon: classified.icon,
    }))
}

/// `run("--list-all")`, deduplicated in first-seen order (`dict.fromkeys`), with
/// blank lines and the `playerctld` proxy itself dropped -- exactly the Python's
/// `if name and name != "playerctld"`.
fn player_names() -> Result<Vec<String>, RunError> {
    let output = run(&["--list-all"])?;
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    for line in output.lines() {
        if line.is_empty() || line == "playerctld" {
            continue;
        }
        if seen.insert(line.to_string()) {
            names.push(line.to_string());
        }
    }
    Ok(names)
}

/// `playing_players()`: every player currently reporting `Playing`, sorted by
/// `(style, player)` -- `sorted()`'s stability and Rust's `sort_by` agree, so ties
/// keep their `player_names()` order.
pub(crate) fn playing_players(browser_titles: &str) -> Result<Vec<PlayerInfo>, RunError> {
    let names = player_names()?;
    let mut playing = Vec::new();
    for name in names {
        if let Some(info) = metadata(&name, browser_titles)? {
            if info.status == "playing" {
                playing.push(info);
            }
        }
    }
    playing.sort_by(|a, b| (a.style, &a.player).cmp(&(b.style, &b.player)));
    Ok(playing)
}

/// `primary_player(players, preferred)`, returning an index into `players` rather
/// than a Python-style object identity: `players` came from [`playing_players`], so
/// every entry has a distinct `player` id and an index picks out exactly the same
/// element `is` would.
///
/// Falls back to `0` when nothing matches, mirroring `players[0]` -- callers only
/// reach this once `players` is known non-empty (`render()`'s `if not players: return
/// payload()` guard), so the fallback is never actually exercised, but staying total
/// here means no `unwrap()`/`expect()` is needed to satisfy it.
pub(crate) fn primary_index(
    players: &[PlayerInfo],
    preferred: &str,
    active: Option<&PlayerInfo>,
) -> usize {
    if let Some(index) = players.iter().position(|player| player.player == preferred) {
        return index;
    }
    if let Some(active) = active {
        let target = (
            active.artist.as_str(),
            active.title.as_str(),
            active.album.as_str(),
        );
        if let Some(index) = players.iter().position(|player| {
            (
                player.artist.as_str(),
                player.title.as_str(),
                player.album.as_str(),
            ) == target
        }) {
            return index;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::{primary_index, PlayerInfo};

    fn player(id: &str, artist: &str, title: &str, album: &str) -> PlayerInfo {
        PlayerInfo {
            player: id.to_string(),
            status: "playing".to_string(),
            artist: artist.to_string(),
            title: title.to_string(),
            album: album.to_string(),
            style: "media",
            source: "Media player".to_string(),
            icon: "",
        }
    }

    #[test]
    fn preferred_wins_when_present() {
        let players = vec![player("a", "", "", ""), player("b", "", "", "")];
        assert_eq!(1, primary_index(&players, "b", None));
    }

    #[test]
    fn preferred_absent_falls_back_to_the_active_track_match() {
        let players = vec![
            player("a", "Artist", "Title", "Album"),
            player("b", "X", "Y", "Z"),
        ];
        let active = player("playerctld", "Artist", "Title", "Album");
        assert_eq!(0, primary_index(&players, "nowhere", Some(&active)));
    }

    #[test]
    fn no_preferred_and_no_active_match_falls_back_to_the_first_player() {
        let players = vec![player("a", "", "", ""), player("b", "", "", "")];
        assert_eq!(0, primary_index(&players, "", None));
    }

    #[test]
    fn an_active_track_that_matches_nobody_still_falls_back_to_the_first_player() {
        let players = vec![player("a", "Artist", "Title", "Album")];
        let active = player("playerctld", "Someone Else", "Other Song", "Other Album");
        assert_eq!(0, primary_index(&players, "", Some(&active)));
    }
}
