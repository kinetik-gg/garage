import QtQuick
import QtQuick.Layouts

// One shortcut in the Keyboard pane: what it does on the left, the combination
// it runs on -- as a button, when it is one the user may move -- on the right.
//
// Not a SettingsRow with a control inside it. A row here is dense and there are
// a hundred of them, so the combination doubles as the control that changes it
// rather than sitting beside a second widget that would say the same thing.
RowLayout {
    id: row
    property string title: ""
    property string subtitle: ""
    property string keys: ""
    property bool editable: true
    property bool modified: false
    property string resetLabel: "Restore"
    property bool resettable: false
    signal edit()
    signal reset()

    Layout.fillWidth: true
    spacing: 12

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 2

        Text {
            Layout.fillWidth: true
            text: row.title
            color: Theme.text
            font.family: Theme.sans
            font.pixelSize: 12
            elide: Text.ElideRight
            maximumLineCount: 1
            renderType: Text.NativeRendering
        }

        Text {
            Layout.fillWidth: true
            visible: row.subtitle !== ""
            text: row.subtitle
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 10
            elide: Text.ElideRight
            maximumLineCount: 1
            renderType: Text.NativeRendering
        }
    }

    Text {
        visible: row.resettable
        text: row.resetLabel
        color: resetPointer.containsMouse ? Theme.text : Theme.textMuted
        font.family: Theme.sans
        font.pixelSize: 11
        renderType: Text.NativeRendering

        MouseArea {
            id: resetPointer
            anchors.fill: parent
            anchors.margins: -6
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: row.reset()
        }
    }

    ContinuousRectangle {
        Layout.preferredWidth: Math.max(96, combination.implicitWidth + 22)
        Layout.preferredHeight: 26
        radius: Theme.controlRadius
        // A changed shortcut is tinted rather than badged: the default is
        // already spelled out underneath, so the row does not need a second
        // thing to read before the combination itself.
        color: !row.editable ? "transparent"
            : keyPointer.containsMouse ? Theme.hoverStrong
            : row.modified ? Theme.hoverStrong : Theme.hover
        borderWidth: 1
        borderColor: row.editable ? Theme.border : Theme.frameInner

        Text {
            id: combination
            anchors.centerIn: parent
            text: row.keys
            color: row.editable ? Theme.text : Theme.textDisabled
            font.family: Theme.sans
            font.pixelSize: 11
            renderType: Text.NativeRendering
        }

        MouseArea {
            id: keyPointer
            anchors.fill: parent
            enabled: row.editable
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: row.edit()
        }
    }
}
