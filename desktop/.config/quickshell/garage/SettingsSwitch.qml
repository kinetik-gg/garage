import QtQuick

// A pill, drawn as a plain Rectangle at half its own height.
//
// This spent a while as a ContinuousRectangle so that its corners came from the
// same superellipse as every panel and button, and followed the corner radius
// setting with them. That is the right rule for a rounded box and the wrong one
// here: a switch is a track with a ball running in it, and a squircle track with
// a circular knob is two different curves in one 38x22 control -- the knob reads
// as rattling in a slot. The two ends match the knob at every radius setting
// instead, which is what makes the thing read as one shape.
Rectangle {
    id: control
    property bool checked: false
    property bool enabled: true
    signal toggled(bool checked)

    width: 38
    height: 22
    radius: height / 2
    color: checked ? Theme.accent : Theme.hoverStrong
    opacity: enabled ? 1 : 0.4
    border.width: checked ? 0 : 1
    border.color: Theme.border
    antialiasing: true

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
