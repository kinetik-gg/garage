import QtQuick

// A segmented picker for small, mutually exclusive choices where seeing every
// option at once matters more than saving width. Sized and coloured to match
// SettingsCombo so the two can sit in the same pane.
ContinuousRectangle {
    id: control

    property var model: []
    property int currentIndex: 0
    property bool enabled: true
    signal activated(int index)

    implicitWidth: 190
    implicitHeight: 32
    radius: Theme.controlRadius
    color: Theme.hover
    borderWidth: 1
    borderColor: Theme.border
    opacity: enabled ? 1 : 0.45

    Row {
        id: segments
        anchors.fill: parent
        anchors.margins: 2

        Repeater {
            model: control.model

            ContinuousRectangle {
                id: segment
                required property int index
                required property string modelData

                width: segments.width / Math.max(control.model.length, 1)
                height: segments.height
                radius: Theme.insetRadius(control.radius, segments.anchors.margins)
                readonly property bool selected: index === control.currentIndex
                color: selected ? Theme.accent
                    : pointer.containsMouse && control.enabled ? Theme.hoverStrong
                    : "transparent"

                Behavior on color {
                    ColorAnimation { duration: 110 }
                }

                Text {
                    anchors.centerIn: parent
                    text: segment.modelData
                    color: segment.selected ? Theme.accentText
                        : control.enabled ? Theme.text : Theme.textDisabled
                    font.family: Theme.sans
                    font.pixelSize: 12
                    font.weight: segment.selected ? Font.DemiBold : Font.Normal
                    renderType: Text.NativeRendering
                }

                MouseArea {
                    id: pointer
                    anchors.fill: parent
                    hoverEnabled: true
                    enabled: control.enabled
                    cursorShape: Qt.PointingHandCursor
                    onClicked: control.activated(segment.index)
                }
            }
        }
    }
}
