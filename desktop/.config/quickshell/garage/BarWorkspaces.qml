import Quickshell.Hyprland
import QtQuick

// The workspace indicator: one dot per workspace on this output, sorted by id,
// widening when active and reddening when a window on it is urgent. Clicking a
// dot activates its workspace -- the ext/workspaces module's own behaviour.
//
// The dots are deliberately not buttons with pills: the old indicator's hover
// feedback lived entirely in the dot itself (idle 45%, hover 65%, active 95%
// opacity of one colour), and that is what this draws.
Item {
    id: workspaces

    readonly property var monitor: Hyprland.monitorFor(barScreen)
    // The screen this bar instance sits on, handed in by the bar.
    property var barScreen: null

    readonly property var entries: {
        const list = monitor && monitor.workspaces ? monitor.workspaces.values : [];
        const sorted = list.slice().sort((a, b) => a.id - b.id);
        return sorted;
    }

    implicitWidth: dotRow.implicitWidth
    implicitHeight: 24

    Row {
        id: dotRow

        anchors.centerIn: parent
        spacing: BarState.scaled("workspaceGap")

        Repeater {
            model: workspaces.entries

            delegate: Item {
                id: dotHolder

                required property var modelData

                readonly property bool active: workspaces.monitor
                    && workspaces.monitor.activeWorkspace
                    ? workspaces.monitor.activeWorkspace.id === modelData.id : false
                // Hover widens the target as well as brightening the dot, so the
                // click area grows to meet the colour instead of trailing it.
                readonly property bool hovered: dotArea.containsMouse

                width: active || hovered ? 20 : 8
                height: 24

                Behavior on width {
                    NumberAnimation {
                        duration: Theme.reduceMotion ? 0 : 180
                        easing.type: Easing.OutCubic
                    }
                }

                Rectangle {
                    anchors.centerIn: parent
                    width: 6
                    height: 6
                    radius: 3
                    color: modelData.urgent ? "#e01b24"
                        : dotHolder.active ? Theme.text
                        : dotHolder.hovered ? Qt.alpha(Theme.text, 0.65)
                        : Qt.alpha(Theme.text, 0.45)

                    Behavior on color {
                        ColorAnimation { duration: Theme.reduceMotion ? 0 : 180 }
                    }
                }

                MouseArea {
                    id: dotArea

                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.LeftButton
                    onClicked: Hyprland.dispatch("workspace " + dotHolder.modelData.id)
                }
            }
        }
    }
}
