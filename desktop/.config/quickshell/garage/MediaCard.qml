import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

// Now playing, with transport. Present only while something is on the MPRIS bus,
// so the control centre has no dead row on a machine that is not playing
// anything -- an invisible item is skipped by the layout above, which is what
// lets the panel shrink back around the rest of the grid.
ContinuousRectangle {
    id: card

    // The singleton owns the playing-over-paused selection rule so this card,
    // the bar chip, launcher actions and hardware keys all target one player.
    readonly property var player: MediaController.player

    readonly property bool isPlaying: card.player !== null && card.player.isPlaying
    readonly property string artUrl: card.player
        ? String(card.player.trackArtUrl || "") : ""

    // The identity, not a placeholder: a player that publishes no metadata at
    // all still tells the user which application is holding the transport.
    readonly property string title: {
        if (!card.player)
            return "";
        const track = String(card.player.trackTitle || "").trim();
        return track !== "" ? track : String(card.player.identity || "Media");
    }

    readonly property string artist: card.player
        ? String(card.player.trackArtist || "").trim() : ""

    function dispatch(role) {
        if (!card.player)
            return;
        if (role === "previous")
            card.player.previous();
        else if (role === "next")
            card.player.next();
        else
            card.player.togglePlaying();
    }

    function allows(role) {
        if (!card.player)
            return false;
        if (role === "previous")
            return card.player.canGoPrevious;
        if (role === "next")
            return card.player.canGoNext;
        return card.player.canTogglePlaying;
    }

    visible: card.player !== null
    implicitHeight: 60
    radius: Theme.cornerRadius
    color: Theme.hover
    borderWidth: 1
    borderColor: Theme.frameInner

    RowLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 10

        // The art, rounded by masking rather than clipping: a clip is a
        // rectangle, and the shell's corners are a superellipse.
        ContinuousRectangle {
            Layout.preferredWidth: 44
            Layout.preferredHeight: 44
            Layout.alignment: Qt.AlignVCenter
            radius: Theme.controlRadius
            color: Theme.iconWell

            Image {
                id: art
                anchors.fill: parent
                source: card.artUrl
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                sourceSize.width: 88
                sourceSize.height: 88
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
                // Not every player publishes art, and a broken url would draw a
                // torn placeholder over the well instead of leaving it empty.
                visible: card.artUrl !== "" && art.status === Image.Ready
                source: art
                maskSource: artMask
                cached: true
            }

            Image {
                id: artFallback
                anchors.centerIn: parent
                width: 20
                height: 20
                source: "icons/speaker-high.svg"
                sourceSize.width: 40
                sourceSize.height: 40
                fillMode: Image.PreserveAspectFit
                smooth: true
                antialiasing: true
                mipmap: true
                visible: false
            }

            ColorOverlay {
                anchors.fill: artFallback
                source: artFallback
                visible: card.artUrl === "" || art.status !== Image.Ready
                color: Theme.iconWellGlyph
                cached: true
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            // Layouts hand a fillWidth item its implicit width first, and a long
            // track title's implicit width is far wider than the panel, which
            // would push the transport off the end before anything elided.
            Layout.preferredWidth: 1
            Layout.minimumWidth: 0
            Layout.alignment: Qt.AlignVCenter
            spacing: 2

            Text {
                Layout.fillWidth: true
                text: card.title
                color: Theme.text
                font.family: Theme.sans
                font.pixelSize: 12
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }

            Text {
                Layout.fillWidth: true
                visible: card.artist !== ""
                text: card.artist
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }
        }

        // One delegate for the three transport buttons. The model is the roles
        // rather than the glyphs, so play/pause swapping its icon changes a
        // binding instead of rebuilding the row.
        Repeater {
            model: ["previous", "toggle", "next"]

            ContinuousRectangle {
                id: control
                required property string modelData

                readonly property string glyphSource: control.modelData === "previous"
                    ? "icons/skip-back.svg"
                    : control.modelData === "next" ? "icons/skip-forward.svg"
                    : card.isPlaying ? "icons/pause.svg" : "icons/play.svg"
                readonly property bool available: card.allows(control.modelData)

                Layout.preferredWidth: 28
                Layout.preferredHeight: 28
                Layout.alignment: Qt.AlignVCenter
                radius: Theme.controlRadius
                color: control.available && controlPointer.containsMouse
                    ? Theme.hoverStrong : "transparent"
                opacity: control.available ? 1 : 0.35

                Image {
                    id: controlGlyph
                    anchors.centerIn: parent
                    width: control.modelData === "toggle" ? 15 : 14
                    height: control.modelData === "toggle" ? 15 : 14
                    source: control.glyphSource
                    sourceSize.width: 32
                    sourceSize.height: 32
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
                    onClicked: card.dispatch(control.modelData)
                }
            }
        }
    }
}
