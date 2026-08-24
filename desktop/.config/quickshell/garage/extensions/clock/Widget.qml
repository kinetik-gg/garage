import QtQuick
import "../.." as Garage

Item {
    id: clockWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    implicitWidth: bar.vertical ? clock.implicitHeight : clock.implicitWidth
    implicitHeight: bar.vertical ? clock.implicitWidth : clock.implicitHeight

    Garage.BarClock {
        id: clock

        anchors.centerIn: parent
        rotation: clockWidget.bar.vertical ? 90 : 0
    }
}
