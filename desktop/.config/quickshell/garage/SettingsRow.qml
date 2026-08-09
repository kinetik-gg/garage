import QtQuick
import QtQuick.Layouts

RowLayout {
    id: row
    property string title: ""
    property string description: ""
    property bool rowEnabled: true
    default property alias controlData: control.data

    Layout.fillWidth: true
    spacing: 22
    opacity: rowEnabled ? 1 : 0.45

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 3

        Text {
            Layout.fillWidth: true
            text: row.title
            color: Theme.text
            font.family: Theme.sans
            font.pixelSize: 13
            font.weight: Font.Medium
            wrapMode: Text.WordWrap
            renderType: Text.NativeRendering
        }

        Text {
            Layout.fillWidth: true
            visible: row.description !== ""
            text: row.description
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 11
            wrapMode: Text.WordWrap
            renderType: Text.NativeRendering
        }
    }

    Item {
        id: control
        Layout.preferredWidth: childrenRect.width
        Layout.preferredHeight: Math.max(32, childrenRect.height)
        Layout.alignment: Qt.AlignVCenter
    }
}
