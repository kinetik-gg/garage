import Quickshell
import Quickshell.Services.Mpris
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

// The standalone now-playing palette: artwork, transport, a seek bar and a
// player switcher, floating over the middle of the bar with a spectrum
// running quietly behind it. MediaCard in the control centre is the compact
// row version of the same Mpris logic; this is the richer surface for
// reaching for while nothing else is open.
//
// A layer surface anchored to the top edge alone, the same idiom
// ScreenshotPalette uses on the bottom edge: a PanelWindow anchored on one
// side only is centred across the rest of the output by the compositor, so
// this lands in the middle of the screen under the bar rather than pinned
// to a corner.
//
// The contract mirrors ControlCenterPalette's: targetScreenName, a
// dismissed() signal, an OnDemand overlay layer surface. holdOpen is new --
// it is what a click-outside dismissal has to check before it closes this
// panel out from under a seek-bar drag, the same way shell.qml's shared
// DismissCatcher already stands down for the screenshot capture
// (`armed: !(... || captureProcess.running)`). Wiring holdOpen into that
// check is next wave's job; this file only has to raise it truthfully.
PanelWindow {
    id: palette

    required property string targetScreenName

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
            if (candidate.name === palette.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    readonly property var players: Mpris.players ? Mpris.players.values : []

    // Pinned by the selector below; falls back to the playing-over-paused
    // rule MediaCard uses -- for the same reason MediaCard uses it, a
    // browser leaving paused tabs registered on the bus -- whenever nothing
    // is pinned, or the pin has dropped off the bus because that player
    // quit or its tab closed.
    property var selectedPlayer: null

    function autoPlayer() {
        for (const candidate of palette.players) {
            if (candidate.isPlaying)
                return candidate;
        }
        return palette.players.length > 0 ? palette.players[0] : null;
    }

    readonly property var player: {
        if (palette.selectedPlayer !== null
            && palette.players.indexOf(palette.selectedPlayer) !== -1)
            return palette.selectedPlayer;
        return palette.autoPlayer();
    }

    readonly property bool isPlaying: palette.player !== null && palette.player.isPlaying
    readonly property string artUrl: palette.player
        ? String(palette.player.trackArtUrl || "") : ""

    readonly property string title: {
        if (!palette.player)
            return "";
        const track = String(palette.player.trackTitle || "").trim();
        return track !== "" ? track : String(palette.player.identity || "Media");
    }

    readonly property string artist: palette.player
        ? String(palette.player.trackArtist || "").trim() : ""

    // The seek bar's own live position. Mpris positionChanged does not fire
    // on a steady tick during playback -- most players only emit it on a
    // seek or a state change -- so the timer below polls it while something
    // is actually playing, and this is reseeded immediately on every other
    // source: a player switch, or a genuine positionChanged from a seek
    // that happened elsewhere (a headset key, another client on the bus).
    property real displayPosition: 0

    function dispatch(role) {
        if (!palette.player)
            return;
        if (role === "previous")
            palette.player.previous();
        else if (role === "next")
            palette.player.next();
        else
            palette.player.togglePlaying();
    }

    function allows(role) {
        if (!palette.player)
            return false;
        if (role === "previous")
            return palette.player.canGoPrevious;
        if (role === "next")
            return palette.player.canGoNext;
        return palette.player.canTogglePlaying;
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

    onPlayerChanged: palette.displayPosition = palette.player ? palette.player.position : 0

    screen: palette.targetScreen()
    implicitWidth: 360
    implicitHeight: Math.max(1, body.implicitHeight + palette.contentMargin * 2)
    color: "transparent"
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    anchors {
        top: true
    }

    margins.top: Theme.windowGutter

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "garage-media"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    // Reattached automatically as palette.player changes identity: Mpris
    // positions do move between events (a seek from another client, a track
    // boundary), and this is what keeps the bar honest between polls.
    Connections {
        target: palette.player
        function onPositionChanged() {
            if (!seekSlider.pressed)
                palette.displayPosition = palette.player.position;
        }
    }

    Timer {
        interval: 500
        repeat: true
        triggeredOnStart: true
        running: palette.visible && palette.isPlaying && !seekSlider.pressed
        onTriggered: palette.displayPosition = palette.player ? palette.player.position : 0
    }

    ContinuousRectangle {
        id: panel
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass, the same as every other overlay
        // namespace in the shell: the material is drawn beneath the
        // surface, so a body colour here would cover it.
        color: Theme.panel
        borderWidth: 1
        borderColor: Theme.frameOuter

        NumberAnimation on opacity {
            from: 0
            to: 1
            duration: 130
            easing.type: Easing.OutCubic
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
        }

        MouseArea {
            anchors.fill: parent
        }

        // Drawn first, so everything else in the panel stacks over it. Kept
        // inside `panel` rather than at the window level so the superellipse
        // clip on the panel's corners clips it too, and bound to the
        // window's own visibility so nothing here samples audio while the
        // palette is not on screen.
        CavaVisualizer {
            id: visualizer
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 1
            height: parent.height * 0.72
            opacity: 0.4
            running: palette.visible
        }

        ColumnLayout {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: palette.contentMargin
            spacing: 10

            Text {
                Layout.fillWidth: true
                Layout.topMargin: 10
                Layout.bottomMargin: 10
                visible: palette.player === null
                horizontalAlignment: Text.AlignHCenter
                text: "No player"
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 13
                renderType: Text.NativeRendering
            }

            RowLayout {
                Layout.fillWidth: true
                visible: palette.player !== null
                spacing: 12

                // The art, rounded by masking rather than clipping -- the
                // same approach MediaCard uses, at a larger size for the
                // standalone panel.
                ContinuousRectangle {
                    Layout.preferredWidth: 64
                    Layout.preferredHeight: 64
                    Layout.alignment: Qt.AlignVCenter
                    radius: Theme.controlRadius
                    color: Theme.iconWell

                    Image {
                        id: art
                        anchors.fill: parent
                        source: palette.artUrl
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        sourceSize.width: 128
                        sourceSize.height: 128
                        smooth: true
                        mipmap: true
                        visible: false
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
                        visible: palette.artUrl !== "" && art.status === Image.Ready
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
                        visible: palette.artUrl === "" || art.status !== Image.Ready
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
                        text: palette.title
                        color: Theme.text
                        font.family: Theme.sans
                        font.pixelSize: 15
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        renderType: Text.NativeRendering
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: palette.artist !== ""
                        text: palette.artist
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
                visible: palette.players.length > 1
                model: palette.players.map(palette.playerLabel)
                currentIndex: Math.max(0, palette.players.indexOf(palette.player))
                onActivated: index => palette.selectedPlayer = palette.players[index]
            }

            RowLayout {
                Layout.fillWidth: true
                visible: palette.player !== null
                spacing: 8
                enabled: palette.player !== null && palette.player.canSeek
                opacity: enabled ? 1 : 0.4

                Text {
                    Layout.preferredWidth: 32
                    text: palette.formatTime(
                        seekSlider.pressed ? seekSlider.value : palette.displayPosition)
                    color: Theme.textMuted
                    font.family: Theme.mono
                    font.pixelSize: 11
                    renderType: Text.NativeRendering
                }

                SettingsSlider {
                    id: seekSlider
                    Layout.fillWidth: true
                    from: 0
                    to: Math.max(1, palette.player && palette.player.lengthSupported
                        ? palette.player.length : 1)
                    value: palette.displayPosition
                    // The far end of the drag alone, not every frame of it:
                    // a seek is one Mpris call, not one per pixel crossed.
                    onCommitted: value => {
                        if (palette.player)
                            palette.player.position = value;
                    }
                }

                Text {
                    Layout.preferredWidth: 32
                    horizontalAlignment: Text.AlignRight
                    text: palette.formatTime(
                        palette.player && palette.player.lengthSupported
                            ? palette.player.length : 0)
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
                visible: palette.player !== null
                spacing: 16

                Repeater {
                    model: ["previous", "toggle", "next"]

                    ContinuousRectangle {
                        id: control
                        required property string modelData

                        readonly property string glyphSource: control.modelData === "previous"
                            ? "icons/skip-back.svg"
                            : control.modelData === "next" ? "icons/skip-forward.svg"
                            : palette.isPlaying ? "icons/pause.svg" : "icons/play.svg"
                        readonly property bool available: palette.allows(control.modelData)

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
                            onClicked: palette.dispatch(control.modelData)
                        }
                    }
                }
            }
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: palette.dismissed()
    }
}
