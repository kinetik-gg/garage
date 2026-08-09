import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // The allocation the backend resolved against the displays actually
    // attached, ids included. Recomputing it here would be a second allocator to
    // disagree with the one that writes the compositor's rules.
    readonly property var plan: controller.snapshot.workspaces || { max: 10, displays: [] }
    readonly property bool shared: controller.preference("workspaces", "mode", "per-display") === "shared"

    readonly property var countLabels: {
        const labels = [];
        for (let count = 1; count <= (plan.max || 10); ++count)
            labels.push(String(count));
        return labels;
    }

    // Whole map, every time. The backend keys the counts by connector name, so a
    // partial write would leave the untouched displays on the default rather
    // than on the number the pane is showing for them.
    function setCount(output, count) {
        const parts = (plan.displays || []).map(item =>
            item.output + "=" + (item.output === output ? count : item.count));
        controller.setPreference("workspaces", "counts", parts.join(","));
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "LAYOUT"

            SettingsRow {
                title: "Workspaces"
                description: pane.shared
                    ? "One set of workspaces, shared by every display. A workspace opens on the display you are using and can be carried to another."
                    : "Every display owns its own workspaces. The number keys always mean this display's workspaces, and a workspace never leaves the display it belongs to."
                SettingsSegmented {
                    model: ["Per Display", "Shared"]
                    currentIndex: pane.shared ? 1 : 0
                    onActivated: index => pane.controller.setPreference(
                        "workspaces", "mode", index === 1 ? "shared" : "per-display")
                }
            }

            SettingsRow {
                title: "Show in Menu Bar"
                description: "Hiding the indicator leaves the workspaces and their shortcuts alone."
                SettingsSwitch {
                    checked: pane.controller.preference("workspaces", "indicator", true)
                    onToggled: value => pane.controller.setPreference(
                        "workspaces", "indicator", value)
                }
            }
        }

        SettingsGroup {
            visible: pane.shared
            title: "COUNT"

            SettingsRow {
                title: "Workspaces"
                description: "Numbered 1 to "
                    + pane.controller.preference("workspaces", "shared_count", 8) + "."
                SettingsCombo {
                    model: pane.countLabels
                    currentIndex: pane.controller.preference("workspaces", "shared_count", 8) - 1
                    onActivated: index => pane.controller.setPreference(
                        "workspaces", "shared_count", index + 1)
                }
            }
        }

        SettingsGroup {
            visible: !pane.shared
            title: "WORKSPACES PER DISPLAY"

            Repeater {
                model: pane.plan.displays || []

                SettingsRow {
                    required property var modelData
                    title: modelData.label
                    // The connector as well as the model name: two identical
                    // panels report the same model, and the ids are the only
                    // thing that tells the rows apart at a glance.
                    description: modelData.output + " · workspaces "
                        + modelData.first + "–" + modelData.last
                    SettingsCombo {
                        model: pane.countLabels
                        currentIndex: modelData.count - 1
                        onActivated: index => pane.setCount(modelData.output, index + 1)
                    }
                }
            }

            // Only when there is genuinely nothing to show. An empty list here
            // means the compositor answered with no displays at all, which is
            // not the same as the pane still loading.
            SettingsRow {
                visible: (pane.plan.displays || []).length === 0
                title: "No displays detected"
                description: "Workspace ranges are handed out to the displays that are attached."
            }
        }

        Text {
            Layout.fillWidth: true
            Layout.leftMargin: 4
            text: pane.shared
                ? "Shared workspaces are numbered once across the whole desktop and are not kept when empty, the way Hyprland behaves on its own. Switching to one that is already open elsewhere moves the focus to that display rather than pulling the workspace over. Going back to Per Display sends every workspace to the display that owns its number, so anything left on 1–"
                    + pane.controller.preference("workspaces", "shared_count", 8)
                    + " lands on the first display."
                : "Each display keeps its own block of numbers for as long as it is known, so changing one display's count never renumbers another, and unplugging a display does not renumber the rest. Adding workspaces leaves everything already open where it is. Removing them moves any windows left over to the last workspace that display keeps — never to another display, even while it is asleep — and sends the display there if it was showing one that went away. The scratchpad is never touched."
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 11
            wrapMode: Text.WordWrap
            renderType: Text.NativeRendering
        }

        Item { Layout.preferredHeight: 20 }
    }
}
