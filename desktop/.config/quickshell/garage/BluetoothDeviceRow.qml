import Quickshell
import Quickshell.Bluetooth
import QtQuick
import QtQuick.Layouts
import Qt5Compat.GraphicalEffects

// One BlueZ device, paired or merely in range.
ColumnLayout {
    id: row
    required property var device

    // Set when the user starts a pairing, so the row can tell a pairing that
    // never began from one that began and fell over.
    property bool attempted: false
    property string notice: ""

    readonly property bool paired: device ? (device.paired || device.bonded) : false
    readonly property bool connected: device ? device.connected : false
    readonly property bool pairing: device ? device.pairing : false
    // BlueZ hands out a freedesktop icon name; it resolves for common classes
    // and comes back empty for the rest, which falls through to the rune glyph.
    readonly property string themeIcon: device && device.icon !== ""
        ? Quickshell.iconPath(device.icon, true) : ""

    Layout.fillWidth: true
    spacing: 8

    function label() {
        if (!device)
            return "";
        return device.name !== "" ? device.name
            : device.deviceName !== "" ? device.deviceName : device.address;
    }

    function detail() {
        if (!device)
            return "";
        const parts = [];
        if (row.pairing) parts.push("Pairing…");
        else parts.push(BluetoothDeviceState.toString(device.state));
        if (row.paired && device.trusted) parts.push("Trusted");
        if (device.batteryAvailable) {
            const percent = device.battery > 1
                ? device.battery : device.battery * 100;
            parts.push("Battery " + Math.round(percent) + "%");
        }
        parts.push(device.address);
        return parts.join(" · ");
    }

    Connections {
        target: row.device

        // pair() reports nothing back, so the only evidence of a refusal is the
        // pairing flag dropping with the device still unpaired.
        function onPairingChanged() {
            if (row.device.pairing)
                return;
            row.notice = row.attempted && !row.device.paired
                ? "Pairing did not complete. A device that asks for a PIN or a passkey "
                    + "needs a Bluetooth pairing agent, which this session does not run — "
                    + "pair it once with bluetoothctl, then manage it from here."
                : "";
            row.attempted = false;
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 12

        Item {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: 18
            Layout.preferredHeight: 18

            Image {
                anchors.fill: parent
                visible: row.themeIcon !== ""
                source: row.themeIcon
                sourceSize.width: 36
                sourceSize.height: 36
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
            }

            Image {
                id: fallbackGlyph
                anchors.fill: parent
                source: "icons/bluetooth.svg"
                sourceSize.width: 36
                sourceSize.height: 36
                fillMode: Image.PreserveAspectFit
                smooth: true
                antialiasing: true
                mipmap: true
                visible: false
            }

            ColorOverlay {
                anchors.fill: fallbackGlyph
                visible: row.themeIcon === ""
                source: fallbackGlyph
                color: row.connected ? Theme.text : Theme.textMuted
                cached: true
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            Text {
                Layout.fillWidth: true
                text: row.label()
                color: Theme.text
                font.family: Theme.sans
                font.pixelSize: 13
                font.weight: row.connected ? Font.DemiBold : Font.Medium
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }

            Text {
                Layout.fillWidth: true
                text: row.detail()
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }
        }

        SettingsButton {
            Layout.alignment: Qt.AlignVCenter
            visible: row.paired && !row.pairing
            text: row.device && row.device.trusted ? "Untrust" : "Trust"
            ghost: true
            onClicked: row.device.trusted = !row.device.trusted
        }

        SettingsButton {
            Layout.alignment: Qt.AlignVCenter
            visible: row.paired && !row.pairing
            text: "Forget"
            ghost: true
            onClicked: row.device.forget()
        }

        SettingsButton {
            Layout.alignment: Qt.AlignVCenter
            text: row.pairing ? "Cancel"
                : !row.paired ? "Pair"
                : row.connected ? "Disconnect" : "Connect"
            prominent: !row.connected && !row.pairing
            onClicked: {
                if (row.pairing) {
                    row.device.cancelPair();
                } else if (!row.paired) {
                    row.notice = "";
                    row.attempted = true;
                    row.device.pair();
                } else if (row.connected) {
                    row.device.disconnect();
                } else {
                    row.device.connect();
                }
            }
        }
    }

    // Where a passkey or PIN confirmation would be surfaced, and where a refused
    // pairing explains itself instead of the row simply snapping back.
    Text {
        Layout.fillWidth: true
        Layout.leftMargin: 30
        visible: row.pairing || row.notice !== ""
        text: row.pairing
            ? "Confirm the pairing request on the device if it asks for one."
            : row.notice
        color: Theme.text
        font.family: Theme.sans
        font.pixelSize: 11
        wrapMode: Text.WordWrap
        renderType: Text.NativeRendering
    }
}
