import QtQuick

// A text chip: the containers counter, the SMB counter. One style for steady
// state, one for a warning, nothing at all when the probe reports unavailable --
// the caller decides by setting `visible`.
Item {
    id: chip

    signal activated()

    property string label: ""
    property bool warning: false
    property string tip: ""
    // The label is usually plain text in the sans face; a chip whose label is a
    // Phosphor codepoint (the AI usage sparkle) names the icon font here, or the
    // codepoint falls through to a CJK fallback and reads as tofu.
    property string labelFont: Theme.sans

    implicitWidth: chipText.implicitWidth + BarState.scaled("module") * 2
    implicitHeight: Math.max(chipText.implicitHeight + 8, 24)

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: clickArea.pressed ? Theme.hoverStrong
            : clickArea.containsMouse ? Theme.hover : "transparent"

        Behavior on color {
            ColorAnimation { duration: Theme.reduceMotion ? 0 : 130 }
        }
    }

    Text {
        id: chipText

        anchors.centerIn: parent
        text: chip.label
        color: chip.warning ? "#e01b24" : Theme.textMuted
        font.family: chip.labelFont
        font.pixelSize: 13
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
