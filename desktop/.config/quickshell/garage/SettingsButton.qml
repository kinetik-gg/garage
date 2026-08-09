import QtQuick
import Qt5Compat.GraphicalEffects

ContinuousRectangle {
    id: button
    property string text: ""
    property string iconSource: ""
    property bool enabled: true
    property bool prominent: false
    // Ghost: no outline, so a secondary action in a dialog reads as subordinate
    // to the prominent one rather than competing with it.
    property bool ghost: false
    property real horizontalPadding: 14
    property real verticalPadding: 8
    signal clicked()

    implicitWidth: content.implicitWidth + horizontalPadding * 2
    implicitHeight: content.implicitHeight + verticalPadding * 2
    radius: Theme.controlRadius
    color: !enabled ? (ghost ? "transparent" : Theme.hover)
        : ghost ? (pointer.containsMouse ? Theme.hoverStrong : "transparent")
        : prominent ? (pointer.containsMouse ? Theme.accentHover : Theme.accent)
        : pointer.containsMouse ? Theme.hoverStrong : Theme.hover
    borderWidth: ghost || prominent ? 0 : 1
    borderColor: enabled ? Theme.border : Theme.frameInner
    opacity: enabled ? 1 : 0.45

    Row {
        id: content
        anchors.centerIn: parent
        spacing: 7

        // Drawn through an overlay so the glyph tracks the label. The svgs ship
        // their own colour, so an Image alone stays fixed while the theme moves.
        Item {
            visible: button.iconSource !== ""
            anchors.verticalCenter: parent.verticalCenter
            width: 16
            height: 16

            Image {
                id: buttonGlyph
                anchors.fill: parent
                source: button.iconSource
                sourceSize.width: 32
                sourceSize.height: 32
                fillMode: Image.PreserveAspectFit
                smooth: true
                antialiasing: true
                mipmap: true
                visible: false
            }

            ColorOverlay {
                anchors.fill: buttonGlyph
                source: buttonGlyph
                color: button.prominent ? Theme.accentText : Theme.text
                cached: true
            }
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: button.text
            color: button.prominent ? Theme.accentText : Theme.text
            font.family: Theme.sans
            font.pixelSize: 12
            font.weight: Font.Medium
            renderType: Text.NativeRendering
        }
    }

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        enabled: button.enabled
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: button.clicked()
    }
}
