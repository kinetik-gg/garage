import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import QtQuick

// The workspace indicator: one dot per workspace on this output, sorted by id,
// widening when active. Clicking a dot activates its workspace -- the
// ext/workspaces module's own behaviour.
//
// The data comes from `hyprctl -j workspaces` and `-j monitors`, the same source
// the settings backend reads, re-run on a debounce after any Hyprland event that
// could move a workspace or the focus. Quickshell's own Hyprland workspace model
// was tried first and is a placeholder on this build -- every entry answers
// id=-1, monitor=null even after an explicit refresh -- so the IPC JSON is what
// the dots trust. Urgent dots are not drawn yet: urgency is per window and
// `workspaces -j` does not carry it.
Item {
    id: workspaces

    // The screen this bar instance sits on, handed in by the bar.
    property var barScreen: null
    readonly property string screenName: barScreen ? barScreen.name : ""

    property var entries: []
    property int activeId: -1

    implicitWidth: dotRow.implicitWidth
    implicitHeight: 24
    visible: entries.length > 0

    Component.onCompleted: workspaces.refresh()

    function refresh() {
        wsProcess.running = true;
        monProcess.running = true;
    }

    Connections {
        target: Hyprland

        function onRawEvent(event) {
            const name = String(event.name || "");
            if (name.startsWith("workspace") || name === "focusedmon"
                || name === "openwindow" || name === "closewindow"
                || name === "movewindow" || name === "urgency")
                debounce.restart();
        }
    }

    // Bursts of events collapse into one refresh 150ms after the last.
    Timer {
        id: debounce

        interval: 150
        onTriggered: workspaces.refresh()
    }

    Process {
        id: wsProcess

        command: ["hyprctl", "-j", "workspaces"]

        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const all = JSON.parse(text);
                    if (!Array.isArray(all))
                        return;
                    workspaces.entries = all.filter(
                        candidate => candidate.monitor === workspaces.screenName);
                } catch (error) {
                    // A compositor mid-reload owes nobody a parse.
                }
            }
        }
    }

    Process {
        id: monProcess

        command: ["hyprctl", "-j", "monitors"]

        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const all = JSON.parse(text);
                    if (!Array.isArray(all))
                        return;
                    const mine = all.find(candidate => candidate.name === workspaces.screenName);
                    workspaces.activeId = mine && mine.activeWorkspace
                        ? Number(mine.activeWorkspace.id) : -1;
                } catch (error) {
                    // Same contract as the workspaces parse above.
                }
            }
        }
    }

    Row {
        id: dotRow

        anchors.centerIn: parent
        spacing: BarState.scaled("workspaceGap")

        Repeater {
            model: workspaces.entries

            delegate: Item {
                id: dotHolder

                required property var modelData

                readonly property bool active: workspaces.activeId === modelData.id
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
                    color: dotHolder.active ? Theme.text
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
