//! `render(preferred="")`: pick the primary player out of everything currently
//! playing and build the text/tooltip/class Waybar shows for it.

use crate::exec::RunError;
use crate::media::browser::BrowserTitleCache;
use crate::media::playerctl::{self, PlayerInfo};
use crate::media::MediaPayload;

/// The two-en-space gap `ICON_GAP` reserves between the icon run and the label --
/// two en spaces rather than ASCII ones, and inside the icon span itself, so the gap
/// scales with the larger glyph size the same way the icons do.
const ICON_GAP: &str = "\u{2002}\u{2002}";

/// `render(preferred="")`. `Ok(MediaPayload::idle())` covers the Python's `if not
/// players: return payload()`; an `Err` only for a spawn failure or a `playerctl`/
/// `hyprctl` timeout, to propagate to an idle payload at the very top of the process
/// exactly as the Python's uncaught `OSError`/`SubprocessError` would.
pub(crate) fn render(
    preferred: &str,
    browser_cache: &BrowserTitleCache,
) -> Result<MediaPayload, RunError> {
    let browser_titles = browser_cache.titles()?;
    let players = playerctl::playing_players(&browser_titles)?;
    if players.is_empty() {
        return Ok(MediaPayload::idle());
    }
    let active = playerctl::metadata("playerctld", "")?;
    let primary_idx = playerctl::primary_index(&players, preferred, active.as_ref());
    let Some(primary) = players.get(primary_idx) else {
        return Ok(MediaPayload::idle());
    };
    let ordered = order_by_primary_first(&players, primary, primary_idx);
    Ok(build_payload(primary, &ordered))
}

/// `[primary, *(player for player in players if player is not primary)]` -- the
/// Python's object-identity exclusion, done here by excluding one specific index
/// rather than comparing values (every `player` id in `players` is already unique, so
/// the two agree, but the index form is the literal analogue of "this object, not an
/// equal one"). Takes `primary` already resolved by the caller rather than indexing
/// `players[primary_idx]` again, so this stays free of `indexing_slicing`.
fn order_by_primary_first<'a>(
    players: &'a [PlayerInfo],
    primary: &'a PlayerInfo,
    primary_idx: usize,
) -> Vec<&'a PlayerInfo> {
    let mut ordered = Vec::with_capacity(players.len());
    ordered.push(primary);
    for (index, player) in players.iter().enumerate() {
        if index != primary_idx {
            ordered.push(player);
        }
    }
    ordered
}

fn build_payload(primary: &PlayerInfo, ordered: &[&PlayerInfo]) -> MediaPayload {
    let details = primary_details(primary);
    let tooltip = build_tooltip(ordered);
    let class = if ordered.len() > 1 {
        "multiple"
    } else {
        primary.style
    };
    MediaPayload {
        text: format!("{}{}", icon_run(ordered), escape_html(&details)),
        tooltip: escape_html(&tooltip),
        class: class.to_string(),
        player: primary.player.clone(),
    }
}

/// `artist — title` when both are present, else whichever of `artist`/`title` is,
/// else the classified source label -- `source_for()` never returns an empty label,
/// so this always has something to show.
fn primary_details(primary: &PlayerInfo) -> String {
    match (primary.artist.is_empty(), primary.title.is_empty()) {
        (false, false) => format!("{} \u{2014} {}", primary.artist, primary.title),
        (false, true) => primary.artist.clone(),
        (true, false) => primary.title.clone(),
        (true, true) => primary.source.clone(),
    }
}

fn build_tooltip(ordered: &[&PlayerInfo]) -> String {
    let mut sections: Vec<String> = ordered
        .iter()
        .map(|player| tooltip_section(player))
        .collect();
    if ordered.len() > 1 {
        sections.insert(0, format!("{} players currently playing", ordered.len()));
    }
    sections.join("\n\n")
}

fn tooltip_section(player: &PlayerInfo) -> String {
    let mut lines = vec![player.source.clone()];
    for field in [&player.artist, &player.title, &player.album] {
        if !field.is_empty() {
            lines.push(field.clone());
        }
    }
    lines.join("\n")
}

/// `icons = "  ".join(player["icon"] for player in ordered)`, then `icon_run =
/// (f'<span font_size="large" rise="-512">{icons}{ICON_GAP}</span>' if icons.strip()
/// else "")`. `icons.strip()` is only ever empty when `ordered` is empty, which
/// [`render`] never calls this with (it returns on an empty `players` before
/// building `ordered` at all); every real [`PlayerInfo`] carries a non-empty Nerd
/// Font glyph from [`crate::media::source::classify`], so in practice this always
/// renders the `<span>` wrapper. The `if icons.strip()` guard is kept anyway, both
/// because the Python has it and because "no glyph, no span" is exactly what an
/// unrecognised player without one would need.
fn icon_run(ordered: &[&PlayerInfo]) -> String {
    let icons = ordered
        .iter()
        .map(|player| player.icon)
        .collect::<Vec<_>>()
        .join("  ");
    if icons.trim().is_empty() {
        String::new()
    } else {
        format!("<span font_size=\"large\" rise=\"-512\">{icons}{ICON_GAP}</span>")
    }
}

/// `html.escape(s, quote=True)`'s exact five replacements, in the exact order
/// `CPython` applies them: `&` first, so the ampersands the later replacements
/// introduce are never themselves re-escaped.
#[must_use]
pub(crate) fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(test)]
mod tests {
    use super::escape_html;

    #[test]
    fn escapes_all_five_characters_html_escape_covers() {
        assert_eq!("&amp;&lt;&gt;&quot;&#x27;", escape_html("&<>\"'"));
    }

    #[test]
    fn ampersand_is_escaped_before_the_others_so_output_is_not_double_escaped() {
        // If '<' were escaped before '&', "<" -> "&lt;" would then have its '&'
        // turned into "&amp;lt;" on a second pass. html.escape does not do that,
        // and neither should this.
        assert_eq!("&lt;", escape_html("<"));
        assert_eq!("&amp;lt;", escape_html("&lt;"));
    }

    #[test]
    fn plain_text_is_untouched() {
        assert_eq!("Artist Name", escape_html("Artist Name"));
    }
}
