import QtQuick 2.15

FocusScope {
    id: field

    property alias text: editor.text
    property string placeholder: ""
    property bool secret: false
    property bool readOnly: false
    property string fontFamily: "Plus Jakarta Sans"

    signal accepted()

    implicitHeight: 46
    activeFocusOnTab: true

    function takeFocus() {
        editor.forceActiveFocus()
    }

    function selectAll() {
        editor.selectAll()
    }

    Rectangle {
        anchors.fill: parent
        radius: 12
        color: "#d9111113"
        border.width: 1
        border.color: editor.activeFocus ? "#0a84ff" : "#293a3a3c"
    }

    Text {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        anchors.verticalCenter: parent.verticalCenter
        visible: editor.text.length === 0
        text: field.placeholder
        color: "#8e8e93"
        elide: Text.ElideRight
        font.family: field.fontFamily
        font.pixelSize: 14
    }

    TextInput {
        id: editor
        anchors.fill: parent
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        verticalAlignment: TextInput.AlignVCenter
        clip: true
        color: "#f5f5f7"
        selectionColor: "#0a84ff"
        selectedTextColor: "#ffffff"
        readOnly: field.readOnly
        echoMode: field.secret ? TextInput.Password : TextInput.Normal
        passwordCharacter: "•"
        font.family: field.fontFamily
        font.pixelSize: 14
        onAccepted: field.accepted()
    }
}
