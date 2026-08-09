import QtQuick
import Qt5Compat.GraphicalEffects

ContinuousRectangle {
    id: card

    required property string iconSource
    required property string title
    required property string shortcut
    // The toolbar sits on glass over arbitrary content, so its foreground is set
    // by the caller rather than following the light/dark palette.
    property color foreground: Theme.text
    property color foregroundMuted: Theme.textMuted
    property real horizontalPadding: 16
    property real verticalPadding: 14
    signal activated()

    implicitWidth: buttonContent.implicitWidth + horizontalPadding * 2
    implicitHeight: buttonContent.implicitHeight + verticalPadding * 2
    radius: Theme.controlRadius
    color: pointer.containsMouse ? Theme.accentSoft : "transparent"

    Row {
        id: buttonContent
        anchors.centerIn: parent
        spacing: 10

        Item {
            anchors.verticalCenter: parent.verticalCenter
            width: 24
            height: 24

            Image {
                id: modeIconSource
                anchors.fill: parent
                source: card.iconSource
                sourceSize.width: 48
                sourceSize.height: 48
                fillMode: Image.PreserveAspectFit
                smooth: true
                antialiasing: true
                mipmap: true
                visible: false
            }

            ColorOverlay {
                anchors.fill: parent
                source: modeIconSource
                color: pointer.containsMouse ? Theme.accentText : card.foreground
                cached: true
            }
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: card.title
            color: pointer.containsMouse ? Theme.accentText : card.foreground
            font.family: Theme.sans
            font.pixelSize: 14
            font.weight: Font.DemiBold
            renderType: Text.NativeRendering
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: card.shortcut
            color: pointer.containsMouse ? Theme.accentText : card.foregroundMuted
            font.family: Theme.sans
            font.pixelSize: 11
            renderType: Text.NativeRendering
        }
    }

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: card.activated()
    }
}
