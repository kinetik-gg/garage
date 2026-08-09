import Quickshell
import Quickshell.Networking
import QtQuick
import QtQuick.Layouts
import Qt5Compat.GraphicalEffects

// One access point in the Wi-Fi list, including its passphrase prompt.
//
// The prompt lives here rather than in the pane so the passphrase never leaves
// the row that asked for it: it goes straight to NetworkManager over DBus
// through connectWithPsk and the field is cleared in the same call. Nothing
// writes it to a preference, a file, or a process argument.
ColumnLayout {
    id: row
    required property var network

    property bool prompting: false
    property string failure: ""

    readonly property int security: network ? network.security : WifiSecurityType.Unknown
    readonly property bool open: security === WifiSecurityType.Open
        || security === WifiSecurityType.Owe
    // 802.1X needs an identity, a method and usually a certificate. A single
    // passphrase field cannot express that, so those go to the editor instead of
    // being half-offered here.
    readonly property bool enterprise: security === WifiSecurityType.Wpa2Eap
        || security === WifiSecurityType.WpaEap
        || security === WifiSecurityType.Wpa3SuiteB192
        || security === WifiSecurityType.DynamicWep
        || security === WifiSecurityType.Leap
    readonly property bool known: network ? network.known : false
    readonly property bool connected: network ? network.connected : false
    readonly property bool busy: network ? network.stateChanging : false

    Layout.fillWidth: true
    spacing: 8

    function securityText(type) {
        switch (type) {
        case WifiSecurityType.Open: return "Open";
        case WifiSecurityType.Owe: return "Open, encrypted";
        case WifiSecurityType.Sae: return "WPA3";
        case WifiSecurityType.Wpa3SuiteB192: return "WPA3 Enterprise";
        case WifiSecurityType.Wpa2Psk: return "WPA2";
        case WifiSecurityType.WpaPsk: return "WPA";
        case WifiSecurityType.Wpa2Eap:
        case WifiSecurityType.WpaEap:
        case WifiSecurityType.Leap:
        case WifiSecurityType.DynamicWep: return "Enterprise";
        case WifiSecurityType.StaticWep: return "WEP";
        default: return "Unknown security";
        }
    }

    // A silent failure here reads as "the button did nothing", so every reason
    // the backend can give gets its own sentence.
    function failureText(reason) {
        switch (reason) {
        case ConnectionFailReason.NoSecrets: return "Wrong password.";
        case ConnectionFailReason.WifiAuthTimeout: return "The network stopped responding while authenticating.";
        case ConnectionFailReason.WifiNetworkLost: return "The network went out of range.";
        case ConnectionFailReason.WifiClientDisconnected: return "The network dropped the connection.";
        case ConnectionFailReason.WifiClientFailed: return "The network refused the connection.";
        default: return "Could not connect (" + ConnectionFailReason.toString(reason) + ").";
        }
    }

    function start() {
        failure = "";
        if (enterprise && !known) {
            Quickshell.execDetached(["nm-connection-editor"]);
            return;
        }
        if (known || open)
            network.connect();
        else
            prompting = true;
    }

    function submit() {
        if (psk.text === "")
            return;
        failure = "";
        prompting = false;
        network.connectWithPsk(psk.text);
        psk.text = "";
    }

    function cancel() {
        prompting = false;
        psk.text = "";
    }

    onPromptingChanged: if (prompting) psk.forceActiveFocus()

    Connections {
        target: row.network

        function onConnectionFailed(reason) {
            row.failure = row.failureText(reason);
            // A saved network with a stale password fails here rather than at the
            // prompt, so this is the only place that can reopen it.
            if (reason === ConnectionFailReason.NoSecrets && !row.open)
                row.prompting = true;
        }

        function onConnectedChanged() {
            if (row.network.connected)
                row.failure = "";
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 12

        SignalBars {
            Layout.alignment: Qt.AlignVCenter
            strength: row.network ? row.network.signalStrength : 0
            muted: !row.connected
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            RowLayout {
                Layout.fillWidth: true
                spacing: 6

                Text {
                    Layout.fillWidth: true
                    text: row.network && row.network.name !== ""
                        ? row.network.name : "Hidden network"
                    color: Theme.text
                    font.family: Theme.sans
                    font.pixelSize: 13
                    font.weight: row.connected ? Font.DemiBold : Font.Medium
                    elide: Text.ElideRight
                    renderType: Text.NativeRendering
                }

                Item {
                    visible: !row.open
                    Layout.preferredWidth: 12
                    Layout.preferredHeight: 12

                    Image {
                        id: lockGlyph
                        anchors.fill: parent
                        source: "icons/lock-simple.svg"
                        sourceSize.width: 24
                        sourceSize.height: 24
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        antialiasing: true
                        mipmap: true
                        visible: false
                    }

                    ColorOverlay {
                        anchors.fill: lockGlyph
                        source: lockGlyph
                        color: Theme.textMuted
                        cached: true
                    }
                }

                Item { Layout.fillWidth: true }
            }

            Text {
                Layout.fillWidth: true
                text: {
                    if (row.busy)
                        return ConnectionState.toString(row.network.state) + "…";
                    const parts = [row.securityText(row.security)];
                    if (row.connected) parts.unshift("Connected");
                    else if (row.known) parts.unshift("Saved");
                    return parts.join(" · ");
                }
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }
        }

        SettingsButton {
            Layout.alignment: Qt.AlignVCenter
            visible: row.known && !row.connected
            text: "Forget"
            ghost: true
            enabled: !row.busy
            onClicked: { row.cancel(); row.network.forget(); }
        }

        SettingsButton {
            Layout.alignment: Qt.AlignVCenter
            text: row.connected ? "Disconnect"
                : row.enterprise && !row.known ? "Set Up…" : "Connect"
            prominent: !row.connected
            enabled: !row.busy
            onClicked: row.connected ? row.network.disconnect() : row.start()
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: 27
        visible: row.prompting
        spacing: 8

        ContinuousRectangle {
            Layout.fillWidth: true
            implicitHeight: 32
            radius: Theme.controlRadius
            color: Theme.hoverStrong
            borderWidth: 1
            borderColor: Theme.border

            TextInput {
                id: psk
                anchors.fill: parent
                anchors.leftMargin: 13
                anchors.rightMargin: 13
                verticalAlignment: Text.AlignVCenter
                echoMode: TextInput.Password
                color: Theme.text
                selectionColor: Theme.accent
                selectedTextColor: Theme.accentText
                font.family: Theme.sans
                font.pixelSize: 12
                selectByMouse: true
                clip: true

                Keys.onReturnPressed: row.submit()
                Keys.onEnterPressed: row.submit()

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: psk.text === ""
                    text: "Password"
                    color: Theme.textDisabled
                    font: psk.font
                    renderType: Text.NativeRendering
                }
            }
        }

        SettingsButton { text: "Cancel"; ghost: true; onClicked: row.cancel() }
        SettingsButton {
            text: "Join"
            prominent: true
            enabled: psk.text !== ""
            onClicked: row.submit()
        }
    }

    Text {
        Layout.fillWidth: true
        Layout.leftMargin: 27
        visible: row.failure !== ""
        text: row.failure
        color: Theme.text
        font.family: Theme.sans
        font.pixelSize: 11
        wrapMode: Text.WordWrap
        renderType: Text.NativeRendering
    }
}
