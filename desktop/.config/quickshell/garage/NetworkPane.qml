import Quickshell
import Quickshell.Networking
import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property bool hasBackend: Networking.backend !== NetworkBackendType.None
    readonly property var deviceList: Networking.devices ? Networking.devices.values : []
    readonly property var wiredDevices: deviceList.filter(item => item.type === DeviceType.Wired)
    // The first Wi-Fi radio. Multiple adapters are rare enough that a second one
    // is better ignored than given a device picker nobody needs.
    readonly property var wifiDevice: deviceList.find(item => item.type === DeviceType.Wifi) || null

    // Connected first, then saved, then by strength -- the order the list is
    // actually read in. Rebuilt whenever an access point appears or vanishes;
    // strength drift alone does not reshuffle rows out from under the pointer.
    readonly property var wifiNetworks: {
        const model = pane.wifiDevice ? pane.wifiDevice.networks : null;
        const list = model ? model.values.slice() : [];
        list.sort((a, b) => (b.connected - a.connected)
            || (b.known - a.known)
            || (b.signalStrength - a.signalStrength));
        return list;
    }

    // Scanning wakes the radio and keeps it awake, so it runs only while this
    // pane is on screen. The device outlives the pane, so the teardown has to be
    // explicit -- a destroyed binding would leave the scanner running.
    function syncScanner(enabled) {
        if (pane.wifiDevice)
            pane.wifiDevice.scannerEnabled = enabled;
    }

    onWifiDeviceChanged: syncScanner(true)
    Component.onCompleted: syncScanner(true)
    Component.onDestruction: syncScanner(false)

    function connectivityText(value) {
        switch (value) {
        case NetworkConnectivity.Full: return "Connected to the internet";
        case NetworkConnectivity.Limited: return "Connected, but the internet is unreachable";
        case NetworkConnectivity.Portal: return "Sign-in required on this network";
        case NetworkConnectivity.None: return "No network connection";
        default: return "Connectivity unknown";
        }
    }

    function wiredText(device) {
        if (device.connected)
            return device.address !== "" ? device.address : "Connected";
        if (device.hasLink)
            return "Cable connected, no address";
        return "No cable connected";
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "STATUS"

            SettingsRow {
                title: pane.connectivityText(Networking.connectivity)
                description: {
                    const online = pane.deviceList.filter(item => item.connected)
                        .map(item => item.name);
                    if (online.length === 0)
                        return "No interface is carrying traffic.";
                    return "Active on " + online.join(", ") + ".";
                }

                SettingsButton {
                    text: "Check Now"
                    enabled: Networking.canCheckConnectivity
                        && Networking.connectivityCheckEnabled
                    iconSource: "icons/arrows-clockwise.svg"
                    onClicked: Networking.checkConnectivity()
                }
            }

            Text {
                Layout.fillWidth: true
                visible: Networking.canCheckConnectivity
                    && !Networking.connectivityCheckEnabled
                text: "NetworkManager's connectivity check is disabled, so the status above "
                    + "reflects the link only, not whether the internet is reachable."
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                wrapMode: Text.WordWrap
                renderType: Text.NativeRendering
            }
        }

        SettingsGroup {
            title: "WIRED"
            visible: pane.wiredDevices.length > 0

            Repeater {
                model: pane.wiredDevices

                SettingsRow {
                    id: wiredRow
                    required property var modelData
                    title: modelData.name
                    description: pane.wiredText(modelData)
                        + (modelData.connected && modelData.linkSpeed > 0
                            ? " · " + modelData.linkSpeed + " Mb/s" : "")

                    SettingsButton {
                        text: "Disconnect"
                        enabled: wiredRow.modelData.connected
                        onClicked: wiredRow.modelData.disconnect()
                    }
                }
            }
        }

        SettingsGroup {
            title: "WI-FI"
            visible: pane.hasBackend

            SettingsRow {
                title: "Wi-Fi"
                description: !Networking.wifiHardwareEnabled
                    ? "The radio is blocked by a hardware switch."
                    : !pane.wifiDevice ? "No Wi-Fi adapter was found."
                    : Networking.wifiEnabled
                        ? pane.wifiDevice.name + " · " + ConnectionState.toString(pane.wifiDevice.state)
                        : "The radio is off."
                rowEnabled: Networking.wifiHardwareEnabled && pane.wifiDevice !== null

                SettingsSwitch {
                    checked: Networking.wifiEnabled
                    enabled: Networking.wifiHardwareEnabled && pane.wifiDevice !== null
                    onToggled: value => Networking.wifiEnabled = value
                }
            }

            Repeater {
                model: Networking.wifiEnabled ? pane.wifiNetworks : []

                WifiNetworkRow {
                    required property var modelData
                    network: modelData
                }
            }

            Text {
                Layout.fillWidth: true
                visible: Networking.wifiEnabled && pane.wifiDevice !== null
                    && pane.wifiNetworks.length === 0
                text: "Looking for networks…"
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                renderType: Text.NativeRendering
            }
        }

        SettingsGroup {
            title: "ADVANCED"
            visible: pane.hasBackend

            SettingsRow {
                title: "Connection Profiles"
                description: "Static addresses, DNS, VPN and 802.1X are edited in "
                    + "NetworkManager's own editor rather than duplicated here."

                SettingsButton {
                    text: "Advanced…"
                    onClicked: Quickshell.execDetached(["nm-connection-editor"])
                }
            }
        }

        Text {
            Layout.fillWidth: true
            visible: !pane.hasBackend
            text: "NetworkManager is not running, so networking cannot be configured here."
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 12
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            renderType: Text.NativeRendering
        }

        Item { Layout.preferredHeight: 20 }
    }
}
