import QtQuick
import "../.." as Garage

Garage.BarIconButton {
    id: menuWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    glyph: "\uf303"
    glyphFamily: "CaskaydiaMono Nerd Font"
    glyphSize: 17
    square: 21
    nudgeRight: bar.vertical ? 0 : 6

    onActivated: bar.openSurface("session", menuWidget)
}
