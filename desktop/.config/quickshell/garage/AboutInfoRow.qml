import QtQuick
import QtQuick.Layouts

RowLayout {
    required property string label
    required property string value

    implicitWidth: 280
    spacing: 12

    Text {
        Layout.preferredWidth: 78
        Layout.alignment: Qt.AlignTop
        text: label
        color: Theme.textMuted
        font.family: Theme.sans
        font.pixelSize: 12
        horizontalAlignment: Text.AlignRight
        renderType: Text.NativeRendering
    }

    Text {
        Layout.preferredWidth: 190
        text: value
        color: Theme.text
        font.family: Theme.sans
        font.pixelSize: 12
        wrapMode: Text.WordWrap
        renderType: Text.NativeRendering
    }
}
