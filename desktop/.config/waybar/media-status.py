#!/usr/bin/env python3
"""Emit source-aware MPRIS metadata for Waybar."""

import html
import json
import subprocess
import sys
import time
from pathlib import Path


PLAYERCTL = "/usr/bin/playerctl"
STATE_FILE = Path.home() / ".cache/waybar-media-player"
SEPARATOR = "\x1f"
METADATA_FORMAT = SEPARATOR.join(
    (
        "{{status}}",
        "{{playerName}}",
        "{{artist}}",
        "{{title}}",
        "{{album}}",
        "{{xesam:url}}",
        "{{mpris:artUrl}}",
    )
)
_browser_titles_cache = (0.0, "")


def output(payload):
    visible = {key: value for key, value in payload.items() if key != "player"}
    print(json.dumps(visible), flush=True)


def payload(text="", tooltip="", style="idle", player=""):
    return {"text": text, "tooltip": tooltip, "class": style, "player": player}


def run(*arguments):
    result = subprocess.run(
        [PLAYERCTL, *arguments],
        capture_output=True,
        text=True,
        timeout=2,
        check=False,
    )
    return result.stdout if result.returncode == 0 else ""


def browser_window_titles():
    global _browser_titles_cache
    now = time.monotonic()
    if now - _browser_titles_cache[0] < 2:
        return _browser_titles_cache[1]
    result = subprocess.run(
        ["/usr/bin/hyprctl", "clients", "-j"],
        capture_output=True,
        text=True,
        timeout=2,
        check=False,
    )
    if result.returncode:
        return _browser_titles_cache[1]
    try:
        clients = json.loads(result.stdout)
    except json.JSONDecodeError:
        return _browser_titles_cache[1]
    titles = []
    for client in clients:
        identity = " ".join(
            str(client.get(field, "")).casefold()
            for field in ("class", "initialClass")
        )
        if any(browser in identity for browser in ("chromium", "chrome", "firefox", "brave", "vivaldi", "zen")):
            titles.append(str(client.get("title", "")))
    joined = " ".join(titles)
    _browser_titles_cache = (now, joined)
    return joined


def metadata(player, browser_titles=""):
    output = run("--player", player, "metadata", "--format", METADATA_FORMAT)
    if not output:
        return None
    fields = output.rstrip("\n").split(SEPARATOR)
    if len(fields) != 7:
        return None
    status, identity, artist, title, album, url, artwork = fields
    style, source, icon = source_for(f"{player} {identity}", url, artwork, browser_titles)
    return {
        "player": player,
        "status": status.casefold(),
        "artist": artist,
        "title": title,
        "album": album,
        "style": style,
        "source": source,
        "icon": icon,
    }


def playing_players():
    names = dict.fromkeys(run("--list-all").splitlines())
    browser_titles = browser_window_titles()
    players = [metadata(name, browser_titles) for name in names if name and name != "playerctld"]
    playing = [player for player in players if player and player["status"] == "playing"]
    return sorted(playing, key=lambda player: (player["style"], player["player"]))


def primary_player(players, preferred=""):
    for player in players:
        if player["player"] == preferred:
            return player
    active = metadata("playerctld")
    if active:
        for player in players:
            if (player["artist"], player["title"], player["album"]) == (
                active["artist"],
                active["title"],
                active["album"],
            ):
                return player
    return players[0]


def render(preferred=""):
    players = playing_players()
    if not players:
        return payload()

    primary = primary_player(players, preferred)
    ordered = [primary, *(
        player for player in players if player is not primary
    )]
    icons = "  ".join(player["icon"] for player in ordered)
    artist = primary["artist"]
    title = primary["title"]
    details = f"{artist} — {title}" if artist and title else artist or title or primary["source"]
    tooltip_sections = []
    for player in ordered:
        lines = [player["source"]]
        lines.extend(player[field] for field in ("artist", "title", "album") if player[field])
        tooltip_sections.append("\n".join(lines))
    if len(players) > 1:
        tooltip_sections.insert(0, f"{len(players)} players currently playing")
    return payload(
        f'<span font_size="large" rise="-1500">{icons}</span>   {html.escape(details)}',
        html.escape("\n\n".join(tooltip_sections)),
        "multiple" if len(players) > 1 else primary["style"],
        primary["player"],
    )


def save_primary(current):
    if not current["player"]:
        return
    STATE_FILE.parent.mkdir(parents=True, exist_ok=True)
    STATE_FILE.write_text(current["player"])


def activate():
    try:
        player = STATE_FILE.read_text().strip()
    except OSError:
        player = render()["player"]
    if not player:
        return

    player = player.casefold()
    if "spotify" in player:
        applications = ("spotify",)
    elif any(browser in player for browser in ("chromium", "chrome")):
        applications = ("google-chrome", "chromium", "brave", "vivaldi")
    elif "firefox" in player:
        applications = ("firefox", "zen")
    elif "vlc" in player:
        applications = ("vlc",)
    elif "mpv" in player:
        applications = ("mpv",)
    else:
        applications = (player.split(".", 1)[0],)

    result = subprocess.run(
        ["/usr/bin/hyprctl", "clients", "-j"],
        capture_output=True,
        text=True,
        timeout=2,
        check=False,
    )
    if result.returncode:
        return
    try:
        clients = json.loads(result.stdout)
    except json.JSONDecodeError:
        return
    for client in reversed(clients):
        identity = " ".join(
            str(client.get(field, "")).casefold()
            for field in ("class", "initialClass", "title", "initialTitle")
        )
        if any(application in identity for application in applications):
            address = client.get("address")
            if address:
                subprocess.run(
                    ["/usr/bin/hyprctl", "dispatch", "focuswindow", f"address:{address}"],
                    timeout=2,
                    check=False,
                )
            return


def source_for(player, url, artwork, browser_titles=""):
    player = player.casefold()
    url = url.casefold()
    artwork = artwork.casefold()
    browser_titles = browser_titles.casefold()
    evidence = " ".join((player, url, artwork))
    is_browser = any(
        browser in player for browser in ("chromium", "chrome", "firefox", "brave", "vivaldi", "zen")
    )

    if "spotify" in evidence:
        return "spotify", "Spotify", ""
    if "music.youtube.com" in url or (is_browser and "youtube music" in browser_titles):
        return "youtube-music", "YouTube Music", ""
    if any(domain in evidence for domain in ("youtube.com", "youtu.be", "ytimg.com")) or (
        is_browser and "youtube" in browser_titles
    ):
        return "youtube", "YouTube", ""
    if is_browser:
        return "browser", "Browser media", ""
    return "media", player or "Media player", ""


try:
    if len(sys.argv) > 1 and sys.argv[1] == "--activate":
        activate()
    else:
        current = render()
        save_primary(current)
        output(current)
except (OSError, subprocess.SubprocessError):
    output(payload())
