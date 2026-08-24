import QtQuick
import "../.." as Garage

Garage.BarTray {
    id: trayWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    spacing: bar.spacing.tray
}
