import Quickshell
import Quickshell.Services.Mpris
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Effects
import QtQuick.Layouts

// The standalone now-playing palette: artwork, transport, a seek bar and a
// player switcher, floating beneath the bar's now-playing readout with a spectrum
// running quietly behind it. MediaCard in the control centre is the compact
// row version of the same Mpris logic; this is the richer surface for
// reaching for while nothing else is open.
//
// The horizontal placement follows MonitorPalette: the bar click supplies a
// monitor-local X, the surface centres itself on that point, and the result is
// clamped into the output. A keyboard launch supplies -1 and uses output centre.
//
// The contract mirrors ControlCenterPalette's: targetScreenName, a
// dismissed() signal, an OnDemand overlay layer surface. holdOpen is new --
// it is what a click-outside dismissal has to check before it closes this
// panel out from under a seek-bar drag, the same way shell.qml's shared
// DismissCatcher already stands down for the screenshot capture
// (`armed: !(... || captureProcess.running)`). Wiring holdOpen into that
// check is next wave's job; this file only has to raise it truthfully.
//
// `id: media`, and it may not go back to being `id: palette`. QQuickItem has
// carried a `palette` property of its own since Qt 6.0, and inside a nested
// component -- a Repeater or Component delegate, which is compiled as its own
// unit with its own creation context -- the identifier resolution order puts
// the delegate item's own properties ahead of the enclosing document's ids. So
// every `palette.*` in the transport delegate below resolved to a QQuickPalette
// instead of to this window: `palette.allows(role)` threw, `available` kept its
// default of false, the delegate's MouseArea was `enabled: false`, and the
// clicks fell straight through it to the panel-filling MouseArea that eats the
// gaps -- three transport buttons that were dim and did nothing. Nothing was
// wrong with the delegate; it was named at the wrong object. Reproduced and
// fixed under qmltestrunner: the same delegate reading `card.*` (MediaCard's
// name, which clashes with nothing) received the click, and so does this one now.
// Ids in the same document are unaffected, which is why the seek bar always
// worked and this looked like a stacking problem rather than a naming one.
PanelWindow {
    id: media

    required property string targetScreenName
    required property real targetAnchorX

    signal dismissed()

    // True for as long as an in-progress gesture must not be interrupted by
    // an outside click -- currently just the seek bar's drag, but written as
    // a general flag rather than "seeking" so a future gesture (the player
    // switcher, say) can raise it without a second property for the next
    // wave to wire up.
    readonly property bool holdOpen: seekSlider.pressed

    readonly property int contentMargin: 14

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === media.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    // playerctld exposes a proxy player whose identity and metadata mirror the
    // real player. Keeping it would produce two identical Spotify/browser
    // segments and send controls through an unnecessary second hop.
    readonly property var players: Mpris.players ? Mpris.players.values.filter(
        candidate => !String(candidate.dbusName || "").endsWith(".playerctld")) : []

    // Pinned by the selector below; falls back to the playing-over-paused
    // rule MediaCard uses -- for the same reason MediaCard uses it, a
    // browser leaving paused tabs registered on the bus -- whenever nothing
    // is pinned, or the pin has dropped off the bus because that player
    // quit or its tab closed.
    property var selectedPlayer: null

    function autoPlayer() {
        for (const candidate of media.players) {
            if (candidate.isPlaying)
                return candidate;
        }
        return media.players.length > 0 ? media.players[0] : null;
    }

    readonly property var player: {
        if (media.selectedPlayer !== null
            && media.players.indexOf(media.selectedPlayer) !== -1)
            return media.selectedPlayer;
        return media.autoPlayer();
    }

    readonly property bool isPlaying: media.player !== null && media.player.isPlaying
    readonly property string artUrl: media.player
        ? String(media.player.trackArtUrl || "") : ""

    readonly property string title: {
        if (!media.player)
            return "";
        const track = String(media.player.trackTitle || "").trim();
        return track !== "" ? track : String(media.player.identity || "Media");
    }

    readonly property string artist: media.player
        ? String(media.player.trackArtist || "").trim() : ""

    // The seek bar's own live position. Mpris positionChanged does not fire
    // on a steady tick during playback -- most players only emit it on a
    // seek or a state change -- so the timer below polls it while something
    // is actually playing, and this is reseeded immediately on every other
    // source: a player switch, or a genuine positionChanged from a seek
    // that happened elsewhere (a headset key, another client on the bus).
    property real displayPosition: 0

    function dispatch(role) {
        if (!media.player)
            return;
        if (role === "previous")
            media.player.previous();
        else if (role === "next")
            media.player.next();
        else
            media.player.togglePlaying();
    }

    function allows(role) {
        if (!media.player)
            return false;
        if (role === "previous")
            return media.player.canGoPrevious;
        if (role === "next")
            return media.player.canGoNext;
        return media.player.canTogglePlaying;
    }

    function playerLabel(candidate) {
        const name = String(candidate.identity || "").trim();
        return name !== "" ? name : "Player";
    }

    function formatTime(seconds) {
        const total = Math.max(0, Math.floor(seconds || 0));
        const minutes = Math.floor(total / 60);
        const secs = total % 60;
        return minutes + ":" + (secs < 10 ? "0" : "") + secs;
    }

    onPlayerChanged: media.displayPosition = media.player ? media.player.position : 0

    screen: media.targetScreen()
    implicitWidth: 360
    implicitHeight: Math.max(1, body.implicitHeight + media.contentMargin * 2)
    color: "transparent"
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    readonly property real surfaceLeft: {
        const target = media.targetScreen();
        const available = target ? target.width : 1920;
        const desired = media.targetAnchorX >= 0
            ? media.targetAnchorX - media.implicitWidth / 2
            : (available - media.implicitWidth) / 2;
        return Math.max(Theme.windowGutter, Math.min(desired,
            available - media.implicitWidth - Theme.windowGutter));
    }

    // Top-to-bottom entrance, shared with every other palette. See PanelMotion.
    PanelMotion {
        id: motion
        onFinished: media.dismissed()
    }

    function requestDismissal() {
        motion.dismiss();
    }

    anchors {
        top: true
        left: true
    }

    margins.top: motion.surfaceTop
    margins.left: media.surfaceLeft

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "garage-media"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    // Reattached automatically as media.player changes identity: Mpris
    // positions do move between events (a seek from another client, a track
    // boundary), and this is what keeps the bar honest between polls.
    Connections {
        target: media.player
        function onPositionChanged() {
            if (!seekSlider.pressed)
                media.displayPosition = media.player.position;
        }
    }

    Timer {
        interval: 500
        repeat: true
        triggeredOnStart: true
        running: media.visible && media.isPlaying && !seekSlider.pressed
        onTriggered: media.displayPosition = media.player ? media.player.position : 0
    }

    ContinuousRectangle {
        id: panel
        opacity: motion.opacity
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass, the same as every other overlay
        // namespace in the shell: the material is drawn beneath the
        // surface, so a body colour here would cover it.
        color: Theme.panel
        borderWidth: 1
        borderColor: Theme.frameOuter


        // The body, over the glass and under everything else. Theme.panel is
        // transparent so the compositor's material shows through, and the
        // material alone is not a readable surface: over a bright window this
        // panel and its text wash out together. Declared before the content so
        // stacking order keeps it underneath without needing a z of its own.
        ContinuousRectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Theme.insetRadius(panel.radius, 1)
            power: Theme.cornerPower
            color: Theme.contentTint
        }

        // Let the current cover colour the glass without replacing it. The
        // source crops to the whole surface -- no letterboxing -- and stays
        // alive while the next asynchronous cover loads, avoiding a blank
        // flash at track boundaries. MultiEffect applies the blur and the
        // superellipse mask in one pass before the result is blended at 30%.
        Item {
            id: artworkBackdrop
            anchors.fill: parent

            property bool hasArtwork: false

            Image {
                id: backdropArt
                anchors.fill: parent
                source: media.artUrl
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                retainWhileLoading: true
                sourceSize.width: 720
                sourceSize.height: 720
                smooth: true
                mipmap: true
                visible: false
                onSourceChanged: {
                    if (source === "")
                        artworkBackdrop.hasArtwork = false;
                }
                onStatusChanged: {
                    if (status === Image.Ready)
                        artworkBackdrop.hasArtwork = true;
                    else if (status === Image.Error || status === Image.Null)
                        artworkBackdrop.hasArtwork = false;
                }
            }

            ContinuousRectangle {
                id: backdropMask
                anchors.fill: parent
                anchors.margins: 1
                radius: Theme.insetRadius(panel.radius, 1)
                power: Theme.cornerPower
                color: "white"
                visible: false
            }

            MultiEffect {
                anchors.fill: parent
                source: backdropArt
                visible: artworkBackdrop.hasArtwork
                opacity: 0.3
                autoPaddingEnabled: false
                blurEnabled: true
                blur: 1.0
                blurMax: 48
                maskEnabled: true
                maskSource: backdropMask
            }
        }

        // Inner hairline one inset px in from the outer one, the double
        // frame every other panel in the shell draws.
        ContinuousRectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Theme.insetRadius(panel.radius, 1)
            power: Theme.cornerPower
            borderWidth: 1
            borderColor: Theme.frameInner
            z: 1
        }

        // The panel eats the clicks that land in the gaps between its controls
        // rather than leaving them unhandled. Declared before everything below
        // it, so it is the lowest thing in the panel and the last offered a
        // click -- which is also how it came to swallow the transport's clicks
        // for as long as those buttons were disabled. See the note on `id: media`
        // at the top of the file.
        MouseArea {
            anchors.fill: parent
        }

        // Capture the live visualizer explicitly, then mask that texture with
        // the panel's superellipse. `hideSource` suppresses only the original
        // scene-graph node; unlike hiding an ancestor Item, it keeps the source
        // renderable for the effect. The mask follows the frame's inside edge.
        CavaVisualizer {
            id: visualizerSource
            anchors.fill: parent
            graphHeight: height
            graphLeftMargin: 2
            graphRightMargin: 2
            // GraphChart keeps half its stroke inside its own bounds, so one
            // logical pixel puts that stroke immediately above the inner frame.
            graphBottomMargin: 1
            running: media.visible
        }

        ShaderEffectSource {
            id: visualizerTexture
            anchors.fill: parent
            sourceItem: visualizerSource
            hideSource: true
            live: true
            recursive: true
            visible: false
        }

        ContinuousRectangle {
            id: visualizerMask
            anchors.fill: parent
            radius: panel.radius
            power: panel.power
            outlineInset: 1
            color: "white"
            visible: false
        }

        OpacityMask {
            anchors.fill: parent
            source: visualizerTexture
            maskSource: visualizerMask
            opacity: 0.4
        }

        ColumnLayout {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: media.contentMargin
            spacing: 10

            Text {
                Layout.fillWidth: true
                Layout.topMargin: 10
                Layout.bottomMargin: 10
                visible: media.player === null
                horizontalAlignment: Text.AlignHCenter
                text: "No player"
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 13
                renderType: Text.NativeRendering
            }

            RowLayout {
                Layout.fillWidth: true
                visible: media.player !== null
                spacing: 12

                // The art, rounded by masking rather than clipping -- the
                // same approach MediaCard uses, at a larger size for the
                // standalone panel.
                ContinuousRectangle {
                    id: artWell
                    Layout.preferredWidth: 64
                    Layout.preferredHeight: 64
                    Layout.alignment: Qt.AlignVCenter
                    radius: Theme.controlRadius
                    color: Theme.iconWell

                    // Keep the last successful frame while the next track's art
                    // loads. Without this, every asynchronous source change swaps
                    // to the fallback for a frame or two and reads as a flicker.
                    property bool hasArtwork: false

                    Image {
                        id: art
                        anchors.fill: parent
                        source: media.artUrl
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        retainWhileLoading: true
                        sourceSize.width: 128
                        sourceSize.height: 128
                        smooth: true
                        mipmap: true
                        visible: false
                        onSourceChanged: {
                            if (source === "")
                                artWell.hasArtwork = false;
                        }
                        onStatusChanged: {
                            if (status === Image.Ready)
                                artWell.hasArtwork = true;
                            else if (status === Image.Error || status === Image.Null)
                                artWell.hasArtwork = false;
                        }
                    }

                    ContinuousRectangle {
                        id: artMask
                        anchors.fill: art
                        radius: Theme.controlRadius
                        color: "white"
                        visible: false
                    }

                    OpacityMask {
                        anchors.fill: art
                        visible: artWell.hasArtwork
                        source: art
                        maskSource: artMask
                        cached: true
                    }

                    Image {
                        id: artFallback
                        anchors.centerIn: parent
                        width: 28
                        height: 28
                        source: "icons/speaker-high.svg"
                        sourceSize.width: 56
                        sourceSize.height: 56
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        antialiasing: true
                        mipmap: true
                        visible: false
                    }

                    ColorOverlay {
                        anchors.fill: artFallback
                        source: artFallback
                        visible: !artWell.hasArtwork
                        color: Theme.iconWellGlyph
                        cached: true
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    Layout.minimumWidth: 0
                    Layout.alignment: Qt.AlignVCenter
                    spacing: 3

                    Text {
                        Layout.fillWidth: true
                        text: media.title
                        color: Theme.text
                        font.family: Theme.sans
                        font.pixelSize: 15
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        renderType: Text.NativeRendering
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: media.artist !== ""
                        text: media.artist
                        color: Theme.textMuted
                        font.family: Theme.sans
                        font.pixelSize: 12
                        elide: Text.ElideRight
                        renderType: Text.NativeRendering
                    }
                }
            }

            // The player switcher: one segment per Mpris player, visible
            // only when there is a choice to make. Segmented rather than a
            // dropdown -- the shell has no chevron glyph, and the control
            // centre's own picker for a small, mutually exclusive set is
            // already this control.
            SettingsSegmented {
                Layout.fillWidth: true
                visible: media.players.length > 1
                model: media.players.map(media.playerLabel)
                currentIndex: Math.max(0, media.players.indexOf(media.player))
                onActivated: index => media.selectedPlayer = media.players[index]
            }

            RowLayout {
                Layout.fillWidth: true
                visible: media.player !== null
                spacing: 8
                enabled: media.player !== null && media.player.canSeek
                opacity: enabled ? 1 : 0.4

                Text {
                    Layout.preferredWidth: 32
                    text: media.formatTime(
                        seekSlider.pressed ? seekSlider.value : media.displayPosition)
                    color: Theme.textMuted
                    font.family: Theme.mono
                    font.pixelSize: 11
                    renderType: Text.NativeRendering
                }

                SettingsSlider {
                    id: seekSlider
                    Layout.fillWidth: true
                    from: 0
                    to: Math.max(1, media.player && media.player.lengthSupported
                        ? media.player.length : 1)
                    value: media.displayPosition
                    // The far end of the drag alone, not every frame of it:
                    // a seek is one Mpris call, not one per pixel crossed.
                    onCommitted: value => {
                        if (media.player)
                            media.player.position = value;
                    }
                }

                Text {
                    Layout.preferredWidth: 32
                    horizontalAlignment: Text.AlignRight
                    text: media.formatTime(
                        media.player && media.player.lengthSupported
                            ? media.player.length : 0)
                    color: Theme.textMuted
                    font.family: Theme.mono
                    font.pixelSize: 11
                    renderType: Text.NativeRendering
                }
            }

            // Transport: previous, toggle, next -- one delegate for the
            // three, the same model-of-roles MediaCard uses so play/pause
            // swapping its glyph is a binding rather than a rebuild.
            RowLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 2
                visible: media.player !== null
                spacing: 16

                Repeater {
                    model: ["previous", "toggle", "next"]

                    ContinuousRectangle {
                        id: control
                        required property string modelData

                        readonly property string glyphSource: control.modelData === "previous"
                            ? "icons/skip-back.svg"
                            : control.modelData === "next" ? "icons/skip-forward.svg"
                            : media.isPlaying ? "icons/pause.svg" : "icons/play.svg"
                        readonly property bool available: media.allows(control.modelData)

                        Layout.preferredWidth: 36
                        Layout.preferredHeight: 36
                        Layout.alignment: Qt.AlignVCenter
                        radius: Theme.controlRadius
                        color: control.available && controlPointer.containsMouse
                            ? Theme.hoverStrong : "transparent"
                        opacity: control.available ? 1 : 0.35

                        Image {
                            id: controlGlyph
                            anchors.centerIn: parent
                            width: control.modelData === "toggle" ? 18 : 17
                            height: control.modelData === "toggle" ? 18 : 17
                            source: control.glyphSource
                            sourceSize.width: 36
                            sourceSize.height: 36
                            fillMode: Image.PreserveAspectFit
                            smooth: true
                            antialiasing: true
                            mipmap: true
                            visible: false
                        }

                        ColorOverlay {
                            anchors.fill: controlGlyph
                            source: controlGlyph
                            color: Theme.text
                            cached: true
                        }

                        MouseArea {
                            id: controlPointer
                            anchors.fill: parent
                            hoverEnabled: true
                            enabled: control.available
                            cursorShape: control.available
                                ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: media.dispatch(control.modelData)
                        }
                    }
                }
            }
        }
    }

    // Through the motion, not straight to dismissed(): the signal is what makes
    // the shell destroy this window, so raising it here would take the panel off
    // screen on the frame Escape lands and leave the exit with nothing to play.
    Shortcut {
        sequence: "Escape"
        onActivated: media.requestDismissal()
    }
}
