import QtQuick

// The track was a plain Rectangle at height/2: a circular-arc pill, the only
// corner in the shell not drawn from the superellipse every neighbour uses, and
// one that ignored the corner radius setting entirely. The knob stays a circle.
ContinuousRectangle {
    id: control
    property bool checked: false
    property bool enabled: true
    signal toggled(bool checked)

    width: 38
    height: 22
    radius: Theme.controlRadius
    color: checked ? Theme.accent : Theme.hoverStrong
    opacity: enabled ? 1 : 0.4
    borderWidth: checked ? 0 : 1
    borderColor: Theme.border

    Rectangle {
        width: 16
        height: 16
        radius: width / 2
        anchors.verticalCenter: parent.verticalCenter
        x: control.checked ? parent.width - width - 3 : 3
        color: control.checked ? Theme.accentText : Theme.textMuted
        antialiasing: true
    }

    MouseArea {
        anchors.fill: parent
        enabled: control.enabled
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: control.toggled(!control.checked)
    }
}
