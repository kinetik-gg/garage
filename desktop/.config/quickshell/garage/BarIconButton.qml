import QtQuick

// A glyph button: the bell, the launcher diamond, the control-centre sliders, the
// session menu. Hover paints the same soft pill every waybar module answered with,
// and a click is one signal up to the bar.
Item {
    id: button

    signal activated()

    // The Phosphor (or Nerd Font) codepoint this button draws.
    property string glyph: ""
    // Phosphor for the icon trio; Caskaydia Nerd for the Arch menu logo.
    property string glyphFamily: "Phosphor"
    property int glyphSize: 16

    implicitWidth: glyphText.implicitWidth + BarState.scaled("icon") * 2
    implicitHeight: Math.max(glyphText.implicitHeight + 8, 24)

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
        id: glyphText

        anchors.centerIn: parent
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
