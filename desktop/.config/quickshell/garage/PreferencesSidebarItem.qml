import QtQuick
import QtQuick.Layouts
import Qt5Compat.GraphicalEffects

ContinuousRectangle {
    id: item
    required property string title
    required property string iconSource
    property bool selected: false
    signal activated()

    implicitWidth: 190
    implicitHeight: 42
    radius: Theme.controlRadius
    color: selected ? Theme.accentSoft : pointer.containsMouse ? Theme.hover : "transparent"

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 10
        anchors.rightMargin: 10
        spacing: 11
        opacity: item.selected ? 1 : 0.5

        Item {
            Layout.preferredWidth: 20
            Layout.preferredHeight: 20

            Image {
                id: iconSource
                anchors.fill: parent
                source: item.iconSource
                sourceSize.width: 40
                sourceSize.height: 40
                smooth: true
                antialiasing: true
                mipmap: true
                visible: false
            }

            ColorOverlay {
                anchors.fill: parent
                source: iconSource
                color: item.selected ? Theme.accentText : Theme.text
                cached: true
            }
        }

        Text {
            Layout.fillWidth: true
            text: item.title
            color: item.selected ? Theme.accentText : Theme.text
            font.family: Theme.sans
            font.pixelSize: 13
            font.weight: Font.Normal
            renderType: Text.NativeRendering
        }
    }

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: item.activated()
    }
}
