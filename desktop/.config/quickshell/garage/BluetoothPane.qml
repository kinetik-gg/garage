import Quickshell.Bluetooth
import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property var adapter: Bluetooth.defaultAdapter
    readonly property bool powered: adapter !== null && adapter.enabled
    readonly property var deviceList: Bluetooth.devices ? Bluetooth.devices.values : []

    function isPaired(device) { return device.paired || device.bonded; }

    readonly property var pairedDevices: {
        const list = pane.deviceList.filter(item => pane.isPaired(item));
        list.sort((a, b) => (b.connected - a.connected));
        return list;
    }

    // Anonymous devices sink to the bottom: an address on its own tells the user
    // nothing, and there are usually more of those than named ones.
    readonly property var nearbyDevices: {
        const list = pane.deviceList.filter(item => !pane.isPaired(item));
        list.sort((a, b) => ((b.name !== "") - (a.name !== ""))
            || String(a.name).localeCompare(String(b.name)));
        return list;
    }

    // Discovery keeps the radio transmitting, so it only runs while this pane is
    // open. The adapter outlives the pane, so the teardown is explicit rather
    // than a binding that would simply stop being evaluated.
    function syncDiscovery(enabled) {
        if (pane.adapter && pane.adapter.enabled)
            pane.adapter.discovering = enabled;
    }

    onPoweredChanged: syncDiscovery(powered)
    Component.onCompleted: syncDiscovery(true)
    Component.onDestruction: syncDiscovery(false)

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "ADAPTER"
            visible: pane.adapter !== null

            SettingsRow {
                title: "Bluetooth"
                description: pane.adapter
                    ? pane.adapter.name + " · "
                        + BluetoothAdapterState.toString(pane.adapter.state) : ""

                SettingsSwitch {
                    checked: pane.powered
                    onToggled: value => pane.adapter.enabled = value
                }
            }

            SettingsRow {
                title: "Discoverable"
                description: pane.adapter && pane.adapter.discoverableTimeout > 0
                    ? "Other devices can find this machine for "
                        + pane.adapter.discoverableTimeout + " seconds."
                    : "Other devices can find this machine."
                rowEnabled: pane.powered

                SettingsSwitch {
                    checked: pane.adapter !== null && pane.adapter.discoverable
                    enabled: pane.powered
                    onToggled: value => pane.adapter.discoverable = value
                }
            }

            SettingsRow {
                title: "Scan For Devices"
                description: pane.adapter && pane.adapter.discovering
                    ? "Scanning. Put the device you want into pairing mode."
                    : "Scanning is off, so new devices will not appear below."
                rowEnabled: pane.powered

                SettingsSwitch {
                    checked: pane.adapter !== null && pane.adapter.discovering
                    enabled: pane.powered
                    onToggled: value => pane.adapter.discovering = value
                }
            }
        }

        SettingsGroup {
            title: "MY DEVICES"
            visible: pane.powered

            Repeater {
                model: pane.pairedDevices

                BluetoothDeviceRow {
                    required property var modelData
                    device: modelData
                }
            }

            Text {
                Layout.fillWidth: true
                visible: pane.pairedDevices.length === 0
                text: "No paired devices yet."
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                renderType: Text.NativeRendering
            }
        }

        SettingsGroup {
            title: "NEARBY"
            visible: pane.powered

            Repeater {
                model: pane.nearbyDevices

                BluetoothDeviceRow {
                    required property var modelData
                    device: modelData
                }
            }

            Text {
                Layout.fillWidth: true
                visible: pane.nearbyDevices.length === 0
                text: pane.adapter && pane.adapter.discovering
                    ? "Looking for devices…"
                    : "Turn scanning on to look for devices."
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                renderType: Text.NativeRendering
            }
        }

        Text {
            Layout.fillWidth: true
            visible: pane.adapter === null
            text: "No Bluetooth adapter was found."
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 12
            horizontalAlignment: Text.AlignHCenter
            renderType: Text.NativeRendering
        }

        Text {
            Layout.fillWidth: true
            visible: pane.adapter !== null && !pane.powered
            text: "Bluetooth is off."
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 12
            horizontalAlignment: Text.AlignHCenter
            renderType: Text.NativeRendering
        }

        Item { Layout.preferredHeight: 20 }
    }
}
