import QtQuick
import QtQuick.Layouts

ContinuousRectangle {
    id: item

    required property string title
    property string shortcut: ""
    property bool itemEnabled: true
    property bool submenu: false
    signal activated()

    implicitWidth: 280
    implicitHeight: 32
    radius: Theme.controlRadius
    color: "transparent"

    ContinuousRectangle {
        anchors.fill: parent
        anchors.margins: 1
        radius: Theme.insetRadius(Theme.controlRadius, 1)
        color: item.itemEnabled && pointer.containsMouse ? Theme.accent : "transparent"
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 9
        anchors.rightMargin: 8
        spacing: 8

        Text {
            Layout.fillWidth: true
            text: item.title
            color: !item.itemEnabled ? Theme.textDisabled
                : pointer.containsMouse ? Theme.accentText : Theme.text
            font.family: Theme.sans
            font.pixelSize: 12
            font.weight: Font.Medium
            renderType: Text.NativeRendering
        }

        Text {
            text: item.submenu ? "›" : item.shortcut
            color: !item.itemEnabled ? Theme.textDisabled
                : pointer.containsMouse ? Theme.accentText : Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 10
            renderType: Text.NativeRendering
        }
    }

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        enabled: item.itemEnabled
        cursorShape: item.itemEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: item.activated()
    }
}
