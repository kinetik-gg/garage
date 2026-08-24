import QtQuick
import "../.." as Garage

Garage.BarMediaChip {
    id: mediaWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    visible: services.media.visible
    onActivated: bar.openSurface("media", mediaWidget)
}
