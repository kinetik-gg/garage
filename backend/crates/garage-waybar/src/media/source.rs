//! `source_for()`: classify a player/track by where the audio is actually coming
//! from, so a browser tab playing `YouTube` reads as "`YouTube`" rather than
//! "`Firefox`".
//!
//! Ported field-for-field from `media-status.py`'s `source_for(player, url, artwork,
//! browser_titles="")`, icons included: each branch's third tuple element is a Nerd
//! Font glyph from the Private Use Area (`U+F0AC`, `U+F1BC`, ...) that a plain-text
//! read of the source can misread as an empty string -- these are real, non-empty
//! values, and [`classify`]'s own tests pin their exact code points so that mistake
//! cannot creep back in silently.

/// One of the five buckets `source_for()` sorts a playing track into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    /// The `class`-ish key used for sort order and the multi-player icon join --
    /// `"spotify"`, `"youtube-music"`, `"youtube"`, `"browser"` or `"media"`.
    pub(crate) style: &'static str,
    /// The human-readable label shown in the tooltip.
    pub(crate) label: String,
    /// The Nerd Font glyph shown in the bar next to (each player's share of) the
    /// icon run.
    pub(crate) icon: &'static str,
}

const BROWSER_MARKERS: [&str; 6] = ["chromium", "chrome", "firefox", "brave", "vivaldi", "zen"];

/// `player.casefold() in evidence` style substring checks, mirrored with
/// `str::to_lowercase()`. Rust's `to_lowercase()` is not a byte-for-byte match for
/// Python's `str.casefold()` on every Unicode input (casefold additionally expands
/// forms like German sharp s) -- a known, low-risk divergence, since every string
/// `source_for()` classifies here is a player identity, a URL or a window title, all
/// of which are ASCII in every real player/browser this bar targets.
#[must_use]
pub(crate) fn classify(player: &str, url: &str, artwork: &str, browser_titles: &str) -> Source {
    let player = player.to_lowercase();
    let url = url.to_lowercase();
    let artwork = artwork.to_lowercase();
    let browser_titles = browser_titles.to_lowercase();
    let evidence = format!("{player} {url} {artwork}");
    let is_browser = BROWSER_MARKERS.iter().any(|marker| player.contains(marker));

    if evidence.contains("spotify") {
        return spotify();
    }
    if url.contains("music.youtube.com") || (is_browser && browser_titles.contains("youtube music"))
    {
        return youtube_music();
    }
    if is_youtube(&evidence) || (is_browser && browser_titles.contains("youtube")) {
        return youtube();
    }
    if is_browser {
        return browser();
    }
    generic(player)
}

fn is_youtube(evidence: &str) -> bool {
    ["youtube.com", "youtu.be", "ytimg.com"]
        .iter()
        .any(|domain| evidence.contains(domain))
}

fn spotify() -> Source {
    Source {
        style: "spotify",
        label: "Spotify".to_string(),
        icon: "\u{f1bc}",
    }
}

fn youtube_music() -> Source {
    Source {
        style: "youtube-music",
        label: "YouTube Music".to_string(),
        icon: "\u{f167}",
    }
}

fn youtube() -> Source {
    Source {
        style: "youtube",
        label: "YouTube".to_string(),
        icon: "\u{f167}",
    }
}

fn browser() -> Source {
    Source {
        style: "browser",
        label: "Browser media".to_string(),
        icon: "\u{f0ac}",
    }
}

/// `player or "Media player"` -- note this is the already-lower-cased `player`
/// string, not the caller's original-case identity, exactly as the Python's local
/// reassignment of `player = player.casefold()` leaves it for this final branch.
fn generic(player: String) -> Source {
    let label = if player.is_empty() {
        "Media player".to_string()
    } else {
        player
    };
    Source {
        style: "media",
        label,
        icon: "\u{f001}",
    }
}

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn spotify_wins_on_player_identity_alone() {
        let source = classify("org.mpris.MediaPlayer2.spotify", "", "", "");
        assert_eq!("spotify", source.style);
        assert_eq!("Spotify", source.label);
        assert_eq!("\u{f1bc}", source.icon);
    }

    #[test]
    fn spotify_evidence_can_come_from_the_url_or_artwork_too() {
        // "evidence" joins player+url+artwork, so Spotify Connect via a browser
        // still classifies as Spotify ahead of the browser check.
        assert_eq!(
            "spotify",
            classify("firefox", "https://open.spotify.com/x", "", "").style
        );
    }

    #[test]
    fn youtube_music_beats_plain_youtube_on_the_url() {
        let source = classify("mpv", "https://music.youtube.com/watch?v=1", "", "");
        assert_eq!("youtube-music", source.style);
        assert_eq!("YouTube Music", source.label);
        assert_eq!("\u{f167}", source.icon);
    }

    #[test]
    fn youtube_music_via_browser_window_title_when_url_is_silent() {
        let source = classify(
            "firefox",
            "",
            "",
            "Some Song - YouTube Music - Mozilla Firefox",
        );
        assert_eq!("youtube-music", source.style);
    }

    #[test]
    fn plain_youtube_from_any_of_its_three_domains() {
        for domain in ["youtube.com", "youtu.be", "ytimg.com"] {
            let url = format!("https://{domain}/watch?v=1");
            let source = classify("mpv", &url, "", "");
            assert_eq!("youtube", source.style, "domain {domain}");
            assert_eq!("\u{f167}", source.icon, "domain {domain}");
        }
    }

    #[test]
    fn plain_youtube_via_browser_window_title() {
        let source = classify("chromium", "", "", "Cat video - YouTube");
        assert_eq!("youtube", source.style);
    }

    #[test]
    fn a_browser_with_no_youtube_or_spotify_evidence_is_just_browser_media() {
        let source = classify("firefox", "https://example.com/", "", "some page title");
        assert_eq!("browser", source.style);
        assert_eq!("Browser media", source.label);
        assert_eq!("\u{f0ac}", source.icon);
    }

    #[test]
    fn every_recognised_browser_name_triggers_the_browser_branch() {
        for name in ["chromium", "chrome", "firefox", "brave", "vivaldi", "zen"] {
            let source = classify(name, "", "", "");
            assert_eq!("browser", source.style, "browser {name}");
        }
    }

    #[test]
    fn a_non_browser_generic_player_falls_back_to_its_own_lowercased_identity() {
        let source = classify("org.mpris.MediaPlayer2.MPV", "", "", "");
        assert_eq!("media", source.style);
        assert_eq!("org.mpris.mediaplayer2.mpv", source.label);
        assert_eq!("\u{f001}", source.icon);
    }

    #[test]
    fn an_empty_generic_identity_falls_back_to_media_player() {
        let source = classify("", "", "", "");
        assert_eq!("media", source.style);
        assert_eq!("Media player", source.label);
    }

    #[test]
    fn every_branchs_icon_is_a_single_non_empty_private_use_area_glyph() {
        // Pinned against a plain-text read of media-status.py misreading these as
        // empty strings (a mistake this port itself made once, caught only by
        // diffing live output against the real Python on a machine with a player
        // actually running).
        for (player, url, expected_codepoint) in [
            ("spotify", "", 0xf1bc),
            ("mpv", "https://music.youtube.com/x", 0xf167),
            ("mpv", "https://youtube.com/x", 0xf167),
            ("firefox", "https://example.com/", 0xf0ac),
            ("mpv", "", 0xf001),
        ] {
            let source = classify(player, url, "", "");
            let mut chars = source.icon.chars();
            let only = chars.next().expect("icon is exactly one character");
            assert!(
                chars.next().is_none(),
                "icon for {player}/{url} has more than one char"
            );
            assert_eq!(expected_codepoint, only as u32, "icon for {player}/{url}");
        }
    }
}
