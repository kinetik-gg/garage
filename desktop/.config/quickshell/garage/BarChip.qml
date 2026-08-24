import QtQuick

// A text chip: the containers counter, the SMB counter, the AI usage
// sparkle. Text sits in the old text-module face -- 13px Plus Jakarta Sans
// 600 at full foreground -- unless the chip overrides size, family or
// colour for a glyph-shaped label.
Item {
    id: chip

    signal activated()

    property string label: ""
    property bool warning: false
    property string tip: ""
    property string labelFont: Theme.sans
    property real labelSize: 13
    property color labelColor: Theme.text

    implicitWidth: chipText.implicitWidth + BarState.scaled("module") * 2
    implicitHeight: Math.max(chipText.implicitHeight + 8, 24)

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: clickArea.pressed ? Qt.alpha(Theme.text, 0.22)
            : clickArea.containsMouse ? Qt.alpha(Theme.text, 0.12) : "transparent"

        Behavior on color {
            ColorAnimation { duration: Theme.reduceMotion ? 0 : 130 }
        }
    }

    Text {
        id: chipText

        anchors.centerIn: parent
        text: chip.label
        color: chip.warning ? Theme.accentPalette.red : chip.labelColor
        font.family: chip.labelFont
        font.pixelSize: chip.labelSize
        font.weight: Font.DemiBold
        renderType: Text.NativeRendering
    }

    MouseArea {
        id: clickArea

        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton
        onClicked: chip.activated()
    }

    BarTip {
        owner: chip
        text: chip.tip
        opacity: chip.tip !== "" && clickArea.containsMouse ? 1 : 0
    }
}
