import QtQuick
import "../.." as Garage

Garage.BarWorkspaces {
    id: workspaceWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    barScreen: bar.screen
    workspaceService: services.workspaces
    vertical: bar.vertical
}
