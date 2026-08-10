import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "POINTER"

            SettingsRow {
                title: "Tracking Speed"
                description: pane.controller.preference("input", "pointer_sensitivity", 0).toFixed(2)
                SettingsSlider {
                    from: -1
                    to: 1
                    stepSize: 0.05
                    value: pane.controller.preference("input", "pointer_sensitivity", 0)
                    onCommitted: next => pane.controller.setPreference(
                        "input", "pointer_sensitivity", Number(next.toFixed(2)))
                }
            }

            SettingsRow {
                title: "Acceleration"
                SettingsCombo {
                    model: ["Flat", "Adaptive"]
                    currentIndex: pane.controller.preference("input", "accel_profile", "flat") === "flat" ? 0 : 1
                    onActivated: index => pane.controller.setPreference(
                        "input", "accel_profile", index === 0 ? "flat" : "adaptive")
                }
            }

            SettingsRow {
                title: "Natural Scrolling"
                description: "Move content in the same direction as your fingers."
                SettingsSwitch {
                    checked: pane.controller.preference("input", "natural_scroll", false)
                    onToggled: value => pane.controller.setPreference("input", "natural_scroll", value)
                }
            }
        }

        SettingsGroup {
            title: "KEYBOARD"

            SettingsRow {
                title: "Keyboard Layout"
                SettingsCombo {
                    model: ["US", "UK", "Indonesian"]
                    currentIndex: Math.max(0, ["us", "gb", "id"].indexOf(
                        pane.controller.preference("input", "keyboard_layout", "us")))
                    onActivated: index => pane.controller.setPreference(
                        "input", "keyboard_layout", ["us", "gb", "id"][index])
                }
            }

            SettingsRow {
                title: "Key Repeat Rate"
                description: Math.round(pane.controller.preference("input", "repeat_rate", 25)) + " per second"
                SettingsSlider {
                    from: 1
                    to: 60
                    stepSize: 1
                    value: pane.controller.preference("input", "repeat_rate", 25)
                    onCommitted: next => pane.controller.setPreference(
                        "input", "repeat_rate", Math.round(next))
                }
            }

            SettingsRow {
                title: "Delay Until Repeat"
                description: Math.round(pane.controller.preference("input", "repeat_delay", 600)) + " ms"
                SettingsSlider {
                    from: 100
                    to: 1500
                    stepSize: 50
                    value: pane.controller.preference("input", "repeat_delay", 600)
                    onCommitted: next => pane.controller.setPreference(
                        "input", "repeat_delay", Math.round(next))
                }
            }
        }

        SettingsGroup {
            visible: pane.controller.snapshot.inputCapabilities.hasTouchpad
            title: "TOUCHPAD"

            SettingsRow {
                title: "Tap to Click"
                SettingsSwitch {
                    checked: pane.controller.preference("input", "touchpad_tap_to_click", true)
                    onToggled: value => pane.controller.setPreference("input", "touchpad_tap_to_click", value)
                }
            }
            SettingsRow {
                title: "Natural Scrolling"
                SettingsSwitch {
                    checked: pane.controller.preference("input", "touchpad_natural_scroll", true)
                    onToggled: value => pane.controller.setPreference("input", "touchpad_natural_scroll", value)
                }
            }
            SettingsRow {
                title: "Disable While Typing"
                SettingsSwitch {
                    checked: pane.controller.preference("input", "touchpad_disable_while_typing", true)
                    onToggled: value => pane.controller.setPreference("input", "touchpad_disable_while_typing", value)
                }
            }
        }

        Item { Layout.preferredHeight: 20 }
    }
}
