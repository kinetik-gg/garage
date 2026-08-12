pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window

    readonly property int sidePadding: 28
    // Upstream injects this context object before loading the QML module.
    // qmllint disable unqualified
    readonly property var agent: hpa
    // qmllint enable unqualified
    property bool blocked: false
    property bool submitted: false

    width: 560
    height: Math.max(320, Math.min(420, content.implicitHeight + sidePadding * 2))
    minimumWidth: width
    maximumWidth: width
    minimumHeight: height
    maximumHeight: height
    visible: true
    color: "transparent"
    flags: Qt.Dialog | Qt.FramelessWindowHint
    modality: Qt.ApplicationModal
    title: "Authentication required"
    font.family: "Plus Jakarta Sans"
    font.pixelSize: 14

    function withAlpha(color, alpha) {
        return Qt.rgba(color.r, color.g, color.b, alpha);
    }

    function displayUser(identity) {
        const value = String(identity);
        const separator = value.lastIndexOf(":");
        return separator >= 0 ? value.slice(separator + 1) : value;
    }

    function cancel() {
        if (submitted)
            return;
        submitted = true;
        agent.setResult("fail");
    }

    function authenticate() {
        if (submitted || blocked || passwordField.text.length === 0)
            return;
        submitted = true;
        blocked = true;
        agent.setResult("auth:" + passwordField.text);
        submitted = false;
    }

    onClosing: event => {
        if (!submitted) {
            event.accepted = false;
            cancel();
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: window.cancel()
    }

    SystemPalette {
        id: system
        colorGroup: SystemPalette.Active
    }

    Rectangle {
        anchors.fill: parent
        radius: 18
        color: window.withAlpha(system.window, 0.98)
        border.width: 1
        border.color: window.withAlpha(system.windowText, 0.16)
    }

    ColumnLayout {
        id: content

        x: window.sidePadding
        y: window.sidePadding
        width: window.width - window.sidePadding * 2
        spacing: 18

        RowLayout {
            Layout.fillWidth: true
            spacing: 14

            Rectangle {
                Layout.preferredWidth: 44
                Layout.preferredHeight: 44
                radius: 13
                color: window.withAlpha(system.highlight, 0.14)
                border.width: 1
                border.color: window.withAlpha(system.highlight, 0.34)

                Rectangle {
                    width: 17
                    height: 14
                    radius: 4
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 9
                    color: system.highlight
                }

                Rectangle {
                    width: 12
                    height: 13
                    radius: 6
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top
                    anchors.topMargin: 8
                    color: "transparent"
                    border.width: 3
                    border.color: system.highlight
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                Label {
                    Layout.fillWidth: true
                    text: "Authentication required"
                    color: system.windowText
                    font.pixelSize: 21
                    font.weight: Font.DemiBold
                }

                Label {
                    Layout.fillWidth: true
                    text: "Authorize as " + window.displayUser(window.agent.getUser())
                    color: window.withAlpha(system.windowText, 0.62)
                    font.pixelSize: 13
                    elide: Text.ElideRight
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            implicitHeight: requestMessage.implicitHeight + 24
            radius: 10
            color: window.withAlpha(system.alternateBase, 0.74)
            border.width: 1
            border.color: window.withAlpha(system.windowText, 0.09)

            Label {
                id: requestMessage

                anchors.fill: parent
                anchors.margins: 12
                text: window.agent.getMessage()
                color: window.withAlpha(system.windowText, 0.78)
                font.pixelSize: 13
                lineHeight: 1.22
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 7

            Label {
                text: "Password"
                color: window.withAlpha(system.windowText, 0.72)
                font.pixelSize: 12
                font.weight: Font.Medium
            }

            TextField {
                id: passwordField

                Layout.fillWidth: true
                Layout.preferredHeight: 44
                leftPadding: 14
                rightPadding: 14
                placeholderText: "Enter your password"
                color: system.text
                placeholderTextColor: window.withAlpha(system.text, 0.42)
                selectionColor: system.highlight
                selectedTextColor: system.highlightedText
                echoMode: TextInput.Password
                passwordCharacter: "●"
                passwordMaskDelay: 0
                focus: true
                readOnly: window.blocked
                persistentSelection: true
                selectByMouse: true
                Keys.onReturnPressed: event => {
                    event.accepted = true;
                    window.authenticate();
                }
                Keys.onEnterPressed: event => {
                    event.accepted = true;
                    window.authenticate();
                }
                onTextEdited: errorLabel.text = ""

                background: Rectangle {
                    radius: 10
                    color: window.withAlpha(system.base, 0.92)
                    border.width: 1
                    border.color: passwordField.activeFocus
                        ? system.highlight
                        : window.withAlpha(system.windowText, 0.14)
                }

                Connections {
                    target: window.agent

                    function onFocusField() {
                        passwordField.forceActiveFocus();
                    }

                    function onBlockInput(block) {
                        window.blocked = block;
                        if (!block) {
                            passwordField.forceActiveFocus();
                            passwordField.selectAll();
                        }
                    }
                }
            }

            Label {
                id: errorLabel

                Layout.fillWidth: true
                Layout.preferredHeight: 18
                color: "#ff6961"
                font.pixelSize: 12
                text: ""
                elide: Text.ElideRight

                Connections {
                    target: window.agent
                    function onSetErrorString(error) {
                        errorLabel.text = error;
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 2
            spacing: 10

            Item {
                Layout.fillWidth: true
            }

            GarageButton {
                text: "Cancel"
                onClicked: window.cancel()
            }

            GarageButton {
                primary: true
                text: window.blocked ? "Authenticating…" : "Authenticate"
                enabled: !window.blocked && passwordField.text.length > 0
                onClicked: window.authenticate()
            }
        }
    }

    component GarageButton: Button {
        id: control

        property bool primary: false

        implicitWidth: Math.max(112, contentItem.implicitWidth + 28)
        implicitHeight: 40
        leftPadding: 14
        rightPadding: 14
        font.pixelSize: 13
        font.weight: Font.DemiBold

        contentItem: Text {
            text: control.text
            color: control.primary
                ? system.highlightedText
                : (control.enabled ? system.buttonText
                                   : window.withAlpha(system.buttonText, 0.42))
            font: control.font
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }

        background: Rectangle {
            radius: 10
            color: control.primary
                ? (control.down ? Qt.darker(system.highlight, 1.12)
                                : system.highlight)
                : (control.down
                    ? window.withAlpha(system.buttonText, 0.14)
                    : (control.hovered
                       ? window.withAlpha(system.buttonText, 0.09)
                       : window.withAlpha(system.buttonText, 0.05)))
            border.width: control.primary || control.activeFocus ? 1 : 0
            border.color: control.primary
                ? window.withAlpha(system.highlightedText, 0.22)
                : window.withAlpha(system.highlight, 0.76)
        }
    }
}
