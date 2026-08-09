import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: group
    property string title: ""
    default property alias groupData: body.data
    Layout.fillWidth: true
    spacing: 9

    Text {
        Layout.leftMargin: 4
        text: group.title
        color: Theme.textMuted
        font.family: Theme.sans
        font.pixelSize: 11
        font.weight: Font.DemiBold
        renderType: Text.NativeRendering
    }

    ContinuousRectangle {
        Layout.fillWidth: true
        implicitHeight: body.implicitHeight + 28
        color: Theme.hover
        borderWidth: 1
        borderColor: Theme.frameInner

        ColumnLayout {
            id: body
            anchors.fill: parent
            anchors.margins: 14
            spacing: 13
        }
    }
}
