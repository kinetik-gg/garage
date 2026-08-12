import QtQuick 2.15

FocusScope {
    id: control

    property string label: ""
    property url iconSource: ""
    property bool primary: false
    property bool ghost: false
    property bool destructive: false
    property bool busy: false
    property string fontFamily: "Plus Jakarta Sans"

    signal clicked()

    implicitHeight: 44
    activeFocusOnTab: enabled
    opacity: enabled ? 1 : 0.45

    function activate() {
        if (enabled && !busy)
            clicked()
    }

    Rectangle {
        anchors.fill: parent
        radius: 12
        border.width: control.primary || control.ghost ? 0 : 1
        border.color: "#293a3a3c"
        color: {
            if (control.primary)
                return control.destructive ? "#ff453a" : "#0a84ff"
            if (control.ghost)
                return pointer.containsMouse || control.activeFocus ? "#143a3a3c" : "transparent"
            return pointer.pressed ? "#3a3a3c" :
                   (pointer.containsMouse || control.activeFocus ? "#2c2c2e" : "#202022")
        }
    }

    Row {
        anchors.centerIn: parent
        spacing: 8

        Image {
            width: 18
            height: 18
            anchors.verticalCenter: parent.verticalCenter
            visible: control.iconSource.toString() !== ""
            source: control.iconSource
            sourceSize.width: 36
            sourceSize.height: 36
            fillMode: Image.PreserveAspectFit
            smooth: true
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: control.busy ? "Signing in…" : control.label
            color: control.primary ? "#ffffff" : "#d1d1d6"
            font.family: control.fontFamily
            font.pixelSize: 14
            font.weight: control.primary ? Font.DemiBold : Font.Medium
        }
    }

    MouseArea {
        id: pointer
        anchors.fill: parent
        enabled: control.enabled && !control.busy
        hoverEnabled: true
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: control.activate()
    }

    Keys.onPressed: function(event) {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter ||
                event.key === Qt.Key_Space) {
            control.activate()
            event.accepted = true
        }
    }
}
