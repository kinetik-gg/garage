import Quickshell
import Quickshell.Io
import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property string aiUsageHelper: Quickshell.env("HOME") + "/.local/bin/garage-ai-usage"

    // Probed once when the pane is built. Loader.setSource recreates this pane
    // on every visit to the section (see PreferencesPalette), so there is no
    // stale state to refresh -- and whether the tokscale CLI exists does not
    // change while Preferences is sitting open.
    property bool aiUsageProbing: true
    property bool aiUsageAvailable: false

    readonly property string background: pane.controller.preference("bar", "background", "blurred")

    Process {
        id: aiUsageProbe
        command: [pane.aiUsageHelper, "--probe"]
        running: true
        onExited: exitCode => {
            pane.aiUsageAvailable = exitCode === 0;
            pane.aiUsageProbing = false;
        }
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "APPEARANCE"

            SettingsRow {
                title: "Background"
                description: pane.background === "transparent"
                    ? "No blur behind the bar." : "Blurs the desktop behind the bar."
                SettingsSegmented {
                    model: ["Blurred", "Transparent"]
                    currentIndex: pane.background === "transparent" ? 1 : 0
                    onActivated: index => pane.controller.setPreference(
                        "bar", "background", index === 1 ? "transparent" : "blurred")
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Height"
                description: Math.round(pane.controller.preference("bar", "height", 44)) + " px"
                SettingsSlider {
                    from: 30
                    to: 60
                    stepSize: 1
                    value: pane.controller.preference("bar", "height", 44)
                    onCommitted: next => pane.controller.setPreference(
                        "bar", "height", Math.round(next))
                }
            }

            SettingsRow {
                title: "Padding"
                description: Number(pane.controller.preference(
                    "bar", "padding_scale", 1.0)).toFixed(2) + "×"
                SettingsSlider {
                    from: 1.0
                    to: 2.0
                    stepSize: 0.05
                    value: pane.controller.preference("bar", "padding_scale", 1.0)
                    onCommitted: next => pane.controller.setPreference(
                        "bar", "padding_scale", next)
                }
            }
        }

        // The workspaces indicator toggle lives on the Workspaces pane, next to
        // the settings that decide what it counts and labels -- it is not
        // duplicated here even though it is also a bar widget.
        SettingsGroup {
            title: "WIDGETS"

            SettingsRow {
                title: "CPU"
                SettingsSwitch {
                    checked: pane.controller.preference("bar", "monitor_cpu", true)
                    onToggled: value => pane.controller.setPreference("bar", "monitor_cpu", value)
                }
            }

            SettingsRow {
                title: "Memory"
                SettingsSwitch {
                    checked: pane.controller.preference("bar", "monitor_memory", true)
                    onToggled: value => pane.controller.setPreference("bar", "monitor_memory", value)
                }
            }

            SettingsRow {
                title: "Network"
                SettingsSwitch {
                    checked: pane.controller.preference("bar", "monitor_network", true)
                    onToggled: value => pane.controller.setPreference("bar", "monitor_network", value)
                }
            }

            SettingsRow {
                title: "Temperature"
                SettingsSwitch {
                    checked: pane.controller.preference("bar", "monitor_temp", true)
                    onToggled: value => pane.controller.setPreference("bar", "monitor_temp", value)
                }
            }

            SettingsRow {
                title: "Disk"
                SettingsSwitch {
                    checked: pane.controller.preference("bar", "monitor_disk", true)
                    onToggled: value => pane.controller.setPreference("bar", "monitor_disk", value)
                }
            }

            SettingsRow {
                title: "GPU"
                SettingsSwitch {
                    checked: pane.controller.preference("bar", "monitor_gpu", true)
                    onToggled: value => pane.controller.setPreference("bar", "monitor_gpu", value)
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            // Disabled rather than hidden when tokscale is missing: the
            // preference the switch writes is still real (garage-ai-usage
            // itself reports {"available": false} and the bar module hides
            // its own text either way), and a control the pane quietly drops
            // would leave a stored "on" nobody can see or change here again
            // once tokscale is installed and the widget starts drawing.
            SettingsRow {
                title: "AI Usage"
                description: pane.aiUsageProbing ? "Checking for tokscale…"
                    : (pane.aiUsageAvailable ? "" : "Install tokscale to enable.")
                SettingsSwitch {
                    enabled: pane.aiUsageAvailable
                    checked: pane.controller.preference("bar", "ai_usage", true)
                    onToggled: value => pane.controller.setPreference("bar", "ai_usage", value)
                }
            }

            SettingsRow {
                title: "Media Player"
                SettingsSwitch {
                    checked: pane.controller.preference("bar", "media_player", true)
                    onToggled: value => pane.controller.setPreference("bar", "media_player", value)
                }
            }
        }

        Item { Layout.preferredHeight: 20 }
    }
}
