import QtQuick
import "../.." as Garage

Garage.BarIconButton {
    id: menuWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    iconSource: Garage.GaragePaths.shellDir + "/icons/archlinux-logo.svg"
    square: 24
    nudgeRight: bar.vertical ? 0 : 6

    onActivated: bar.openSurface("session", menuWidget)
}
