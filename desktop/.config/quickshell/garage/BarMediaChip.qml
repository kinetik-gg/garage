import QtQuick

// The media chip: source glyph and "artist — title", the old module's whole
// readout. Middle-click toggles playback, right-click skips forward, and the
// wheel steps tracks in the direction the old module's scroll bindings did --
// up for previous, down for next. There is no play/pause glyph in the bar:
// the old module never drew one, and the transport lives in the media panel.
Item {
    id: mediaChip

    signal activated()

    implicitWidth: row.implicitWidth + BarState.scaled("module") * 2
    implicitHeight: Math.max(row.implicitHeight + 8, 24)
    visible: MediaController.visible

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: clickArea.pressed ? Qt.alpha(Theme.text, 0.22)
            : clickArea.containsMouse ? Qt.alpha(Theme.text, 0.12) : "transparent"

        Behavior on color {
            ColorAnimation { duration: Theme.reduceMotion ? 0 : 130 }
        }
    }

    Row {
        id: row

        anchors.centerIn: parent
        spacing: 8

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: MediaController.iconGlyph
            color: Theme.text
            font.family: "Caskaydia Mono Nerd Font Mono"
            font.pixelSize: 16
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
