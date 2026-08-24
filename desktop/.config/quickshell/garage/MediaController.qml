pragma Singleton
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Services.Mpris
import QtQuick

// The media readout's state, event-driven end to end.
//
// The old external module polled a command-line controller and `hyprctl clients` twice
// a second; this reads the MPRIS bus directly and keeps its browser-title evidence
// current from Hyprland's own window events. Nothing here wakes up on a timer.
//
// The classification order is the old module's source table: Spotify beats YouTube
// Music, YouTube Music beats plain YouTube, and a browser beats anything generic --
// decided per player from the bus name, the track URL, the artwork URL and (for
// browsers) the titles of the browser windows that exist right now.
Singleton {
    id: media

    // -- Players -------------------------------------------------------------

    readonly property var players: Mpris.players ? Mpris.players.values : []

    // Whatever is playing, else the first player -- the MediaCard rule, kept identical
    // so the chip and the palettes can never disagree about what is playing.
    readonly property var player: {
        for (let index = 0; index < players.length; ++index)
            if (players[index].isPlaying)
                return players[index];
        return players.length > 0 ? players[0] : null;
    }

    readonly property bool isPlaying: player !== null && player.isPlaying
    readonly property bool visible: players.length > 0

    readonly property string title: {
        if (!player)
            return "";
        const track = String(player.trackTitle || "").trim();
        return track !== "" ? track : String(player.identity || "Media");
    }
    readonly property string artist: player ? String(player.trackArtist || "").trim() : ""

    function togglePlaying() {
        if (player && player.canTogglePlaying)
            player.togglePlaying();
    }

    function play() {
        if (player && player.canPlay)
            player.play();
    }

    function pause() {
        if (player && player.canPause)
            player.pause();
    }

    function stop() {
        // MprisPlayer has no separate canStop flag; CanControl is the MPRIS
        // capability that covers Stop.
        if (player && player.canControl)
            player.stop();
    }

    function next() {
        if (player && player.canGoNext)
            player.next();
    }

    function previous() {
        if (player && player.canGoPrevious)
            player.previous();
    }

    // One native action seam for launcher and IPC callers. The launcher keeps
    // "skip" as its user-facing action name; both spellings intentionally
    // reach the same MPRIS Next call.
    function dispatch(action) {
        if (action === "play")
            media.play();
        else if (action === "pause")
            media.pause();
        else if (action === "stop")
            media.stop();
        else if (action === "toggle" || action === "play-pause")
            media.togglePlaying();
        else if (action === "skip" || action === "next")
            media.next();
        else if (action === "previous")
            media.previous();
    }

    // -- Browser title evidence ----------------------------------------------

    readonly property var browserClasses: [
        "chromium", "chrome", "google-chrome", "brave", "firefox", "vivaldi", "zen"
    ]

    // address -> { class, title }, maintained from Hyprland events plus one snapshot.
    property var browserTitles: ({})

    readonly property string browserTitleText: {
        const parts = [];
        for (const address in browserTitles) {
            const entry = browserTitles[address];
            if (entry.title !== "")
                parts.push(entry.title);
        }
        return parts.join(" ").toLowerCase();
    }

    Component.onCompleted: requestClients.running = true

    Process {
        id: requestClients
        command: ["hyprctl", "-j", "clients"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const clients = JSON.parse(text);
                    if (!Array.isArray(clients))
                        return;
                    const next = {};
                    for (let index = 0; index < clients.length; ++index) {
                        const client = clients[index];
                        const klass = String(client.class || "").toLowerCase();
                        const initial = String(client.initialClass || "").toLowerCase();
                        const isBrowser = media.browserClasses.some(candidate =>
                            klass.indexOf(candidate) !== -1
                            || initial.indexOf(candidate) !== -1);
                        if (!isBrowser)
                            continue;
                        next[String(client.address || "")] = {
                            class: klass,
                            title: String(client.title || "")
                        };
                    }
                    browserTitles = next;
                } catch (error) {
                    // A compositor mid-reload owes nobody a parse.
                }
            }
        }
    }

    Connections {
        target: Hyprland

        function onRawEvent(event) {
            // openwindow>>ADDR,... and windowtitle>>ADDR,TITLE both lead with the
            // address; closewindow>>ADDR carries it alone.
            const address = String(event.data || "").split(",")[0] || "";
            if (event.name === "closewindow") {
                if (address in browserTitles) {
                    const next = Object.assign({}, browserTitles);
                    delete next[address];
                    browserTitles = next;
                }
                return;
            }
            if (event.name === "openwindow" || event.name === "windowtitle") {
                // Title events arrive before the compositor's state settles; a short
                // debounce turns the burst into one hyprctl read.
                titleDebounce.restart();
            }
        }
    }

    property Timer titleDebounce: Timer {
        interval: 150
        onTriggered: requestClients.running = true
    }

    // -- Classification ------------------------------------------------------

    function classification(playerCandidate) {
        if (!playerCandidate)
            return { style: "generic", label: "Media player" };
        const bus = String(playerCandidate.dbusName || "").toLowerCase();
        const url = String(playerCandidate.trackUrl || "");
        const art = String(playerCandidate.trackArtUrl || "");
        const identity = String(playerCandidate.identity || "").toLowerCase();
        const musicEvidence = url + " " + art + " " + String(playerCandidate.trackTitle || "");

        if (bus.indexOf("spotify") !== -1 || identity.indexOf("spotify") !== -1)
            return { style: "spotify", label: "Spotify" };
        if (musicEvidence.indexOf("music.youtube.com") !== -1
                || browserTitleText.indexOf("youtube music") !== -1)
            return { style: "youtube-music", label: "YouTube Music" };
        if (musicEvidence.indexOf("youtube.com") !== -1
                || musicEvidence.indexOf("youtu.be") !== -1
                || browserTitleText.indexOf("youtube") !== -1)
            return { style: "youtube", label: "YouTube" };
        if (browserClasses.some(candidate =>
                (bus + " " + identity).indexOf(candidate) !== -1))
            return { style: "browser", label: "Browser media" };
        return { style: "generic",
            label: identity !== "" ? identity : "Media player" };
    }

    readonly property var classified: classification(player)

    // Caskaydia Mono Nerd Font carries the brand codepoints the old readout drew;
    // Phosphor deliberately ships no brand logos.
    readonly property string iconGlyph: classified.style === "spotify" ? "\uf1bc"
        : classified.style === "youtube-music" || classified.style === "youtube" ? "\uf167"
        : classified.style === "browser" ? "\uf0ac" : "\uf001"

    // The readout text: icon run, then "artist — title", falling back to whichever
    // half exists, then to the source label. The em dash is the old module's.
    readonly property string labelGlyphs:
        isPlaying ? "\u25b6" : "\u23f8"

    function detailText(sourceLabel) {
        if (artist !== "" && title !== "")
            return artist + " \u2014 " + title;
        if (title !== "")
            return title;
        if (artist !== "")
            return artist;
        return sourceLabel;
    }
}
