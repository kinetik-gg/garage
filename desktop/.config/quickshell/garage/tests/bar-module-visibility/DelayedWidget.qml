import QtQuick

Item {
    required property var bar
    required property var services
    required property var manifest
    required property var probe

    visible: services.ready
    implicitWidth: 96
    implicitHeight: 24
}
