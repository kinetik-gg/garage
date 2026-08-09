import QtQuick
import QtQuick.Layouts

Item {
    Layout.fillWidth: true
    Layout.leftMargin: 6
    Layout.rightMargin: 6
    implicitHeight: 2

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 1
        color: Theme.frameOuter
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 1
        color: Theme.frameInner
    }
}
