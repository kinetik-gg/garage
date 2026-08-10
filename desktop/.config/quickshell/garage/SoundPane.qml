import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    function defaultDevice(items) {
        for (const item of items || []) if (item.default) return item;
        return items && items.length ? items[0] : null;
    }

    function descriptions(items) {
        return (items || []).map(item => item.description);
    }

    function deviceValue(items, key, fallback) {
        const device = defaultDevice(items);
        return device && device[key] !== undefined ? device[key] : fallback;
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "OUTPUT"

            SettingsRow {
                title: "Output Device"
                rowEnabled: pane.controller.snapshot.audio.outputs.length > 0
                SettingsCombo {
                    model: pane.descriptions(pane.controller.snapshot.audio.outputs)
                    currentIndex: Math.max(0, pane.controller.snapshot.audio.outputs.findIndex(item => item.default))
                    onActivated: index => pane.controller.action(
                        "audio.output.default", pane.controller.snapshot.audio.outputs[index].name)
                }
            }

            SettingsRow {
                title: "Output Volume"
                description: Math.round(pane.deviceValue(
                    pane.controller.snapshot.audio.outputs, "volume", 0) * 100) + "%"
                rowEnabled: pane.defaultDevice(pane.controller.snapshot.audio.outputs) !== null
                Row {
                    spacing: 10
                    SettingsSlider {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 150
                        value: pane.deviceValue(pane.controller.snapshot.audio.outputs, "volume", 0)
                        onCommitted: next => pane.controller.action("audio.output.volume", next)
                    }
                    SettingsButton {
                        anchors.verticalCenter: parent.verticalCenter
                        text: pane.deviceValue(pane.controller.snapshot.audio.outputs, "mute", false) ? "Unmute" : "Mute"
                        iconSource: pane.deviceValue(pane.controller.snapshot.audio.outputs, "mute", false)
                            ? "icons/speaker-high.svg" : "icons/speaker-slash.svg"
                        onClicked: pane.controller.action("audio.output.mute",
                            !pane.deviceValue(pane.controller.snapshot.audio.outputs, "mute", false))
                    }
                }
            }
        }

        SettingsGroup {
            title: "INPUT"

            SettingsRow {
                title: "Input Device"
                rowEnabled: pane.controller.snapshot.audio.inputs.length > 0
                SettingsCombo {
                    model: pane.descriptions(pane.controller.snapshot.audio.inputs)
                    currentIndex: Math.max(0, pane.controller.snapshot.audio.inputs.findIndex(item => item.default))
                    onActivated: index => pane.controller.action(
                        "audio.input.default", pane.controller.snapshot.audio.inputs[index].name)
                }
            }

            SettingsRow {
                title: "Input Volume"
                description: Math.round(pane.deviceValue(
                    pane.controller.snapshot.audio.inputs, "volume", 0) * 100) + "%"
                rowEnabled: pane.defaultDevice(pane.controller.snapshot.audio.inputs) !== null
                Row {
                    spacing: 10
                    SettingsSlider {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 150
                        value: pane.deviceValue(pane.controller.snapshot.audio.inputs, "volume", 0)
                        onCommitted: next => pane.controller.action("audio.input.volume", next)
                    }
                    SettingsButton {
                        anchors.verticalCenter: parent.verticalCenter
                        text: pane.deviceValue(pane.controller.snapshot.audio.inputs, "mute", false) ? "Unmute" : "Mute"
                        iconSource: pane.deviceValue(pane.controller.snapshot.audio.inputs, "mute", false)
                            ? "icons/speaker-high.svg" : "icons/speaker-slash.svg"
                        onClicked: pane.controller.action("audio.input.mute",
                            !pane.deviceValue(pane.controller.snapshot.audio.inputs, "mute", false))
                    }
                }
            }
        }

        Text {
            Layout.fillWidth: true
            visible: pane.controller.snapshot.audio.outputs.length === 0
                && pane.controller.snapshot.audio.inputs.length === 0
            text: "PipeWire audio devices are unavailable."
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 12
            horizontalAlignment: Text.AlignHCenter
            renderType: Text.NativeRendering
        }

        Item { Layout.preferredHeight: 20 }
    }
}
