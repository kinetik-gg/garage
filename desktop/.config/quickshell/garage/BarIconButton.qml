import QtQuick

// A glyph button: the bell, the launcher diamond, the control-centre
// sliders, the session menu. Hover and press paint the old module tints --
// 12% of the foreground on hover, 22% while pressed -- at the old 8px
// radius. Hover changes colour; it never changes geometry.
Item {
    id: button

    signal activated()

    // The Phosphor (or Nerd Font) codepoint this button draws.
    property string glyph: ""
    // Phosphor for the icon trio; Caskaydia Nerd for the Arch menu logo.
    property string glyphFamily: "Phosphor"
    property int glyphSize: 16
    // The Arch menu button is a fixed square with an optical nudge on its
    // right, exactly as the old stylesheet drew it; 0 keeps the natural
    // glyph-plus-padding sizing for the icon trio.
    property int square: 0
    property int nudgeRight: 0

    implicitWidth: square > 0 ? square + nudgeRight
        : glyphText.implicitWidth + BarState.scaled("icon") * 2
    implicitHeight: square > 0 ? square
        : Math.max(glyphText.implicitHeight + 8, 24)

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
        id: glyphText

        anchors.centerIn: parent
        // With a right nudge the glyph centres in the square, not the square
        // plus nudge -- the nudge is empty space by design.
        anchors.horizontalCenterOffset: -button.nudgeRight / 2
        text: button.glyph
        color: Theme.text
        font.family: button.glyphFamily
        font.pixelSize: button.glyphSize
        font.weight: Font.DemiBold
        renderType: Text.NativeRendering
    }

    MouseArea {
        id: clickArea

        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton
        onClicked: button.activated()
    }
}
