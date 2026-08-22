import QtQuick

// The media chip: source glyph, transport state, artist — title. Middle-click
// toggles playback, right-click skips forward, and the wheel steps tracks in the
// direction the old module's scroll bindings did -- up for previous, down for next.
Item {
    id: mediaChip

    signal activated()

    implicitWidth: row.implicitWidth + BarState.scaled("module") * 2
    implicitHeight: Math.max(row.implicitHeight + 8, 24)
    visible: MediaController.visible

    Row {
        id: row

        anchors.centerIn: parent
        spacing: 8

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: MediaController.iconGlyph
            color: Theme.textMuted
            font.family: "Caskaydia Mono Nerd Font Mono"
            font.pixelSize: 14
            renderType: Text.NativeRendering
        }

        // Play/pause as glyphs rather than icons: they follow the bar's colour,
        // and the transport is the one part that must read at a glance.
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: MediaController.isPlaying ? "\u25b6" : "\u23f8"
            color: MediaController.isPlaying ? Theme.accent : Theme.textMuted
            font.family: "Phosphor"
            font.pixelSize: 13
            renderType: Text.NativeRendering
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: {
                const detail = MediaController.detailText(MediaController.classified.label);
                return detail.length > 42 ? detail.slice(0, 41) + "…" : detail;
            }
            color: Theme.text
            font.family: Theme.sans
            font.pixelSize: 13
            font.weight: Font.DemiBold
            renderType: Text.NativeRendering

            // The variable width the old module's label had; the palette anchors
            // under wherever this actually lands.
            onImplicitWidthChanged: mediaChip.implicitWidthChanged()
        }
    }

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: clickArea.pressed ? Theme.hoverStrong
            : clickArea.containsMouse ? Theme.hover : "transparent"

        Behavior on color {
            ColorAnimation { duration: Theme.reduceMotion ? 0 : 130 }
        }
    }

    MouseArea {
        id: clickArea

        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton

        onClicked: mouse => {
            if (mouse.button === Qt.MiddleButton)
                MediaController.togglePlaying();
            else if (mouse.button === Qt.RightButton)
                MediaController.next();
            else
                mediaChip.activated();
        }

        WheelHandler {
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: event => {
                if (event.angleDelta.y > 0)
                    MediaController.previous();
                else if (event.angleDelta.y < 0)
                    MediaController.next();
                event.accepted = true;
            }
        }
    }

    BarTip {
        owner: mediaChip
        text: MediaController.visible
            ? MediaController.classified.label + "\n" + MediaController.detailText("")
            : ""
        opacity: MediaController.visible && clickArea.containsMouse ? 1 : 0
    }
}
