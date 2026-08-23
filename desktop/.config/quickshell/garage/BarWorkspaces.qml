import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import QtQuick

// The workspace indicator, drawn to the old indicator's own spec.
//
// Geometry and colour come from the deleted waybar stylesheet, via the spacing
// table BarState carries: an idle dot is 6x6 at 45% of the foreground, hover
// raises the dot to 65% and paints a soft 8px-radius tint behind the button,
// and the ACTIVE workspace is a 20x6 stadium at 95% -- a pill, not a dot. The
// pill's width animates over 180ms; that is the only width in the row that
// ever changes, because a width that answers to hover shoves every dot to its
// right around -- the layout shift this indicator is forbidden to have. Hover
// is colour-only.
//
// Data comes from `hyprctl -j workspaces` and `-j monitors`, the same source
// the settings backend reads, re-run on a debounce after any Hyprland event
// that could move a workspace or the focus. Quickshell's own Hyprland
// workspace model is a placeholder on this build -- every entry answers
// id=-1, monitor=null even after an explicit refresh -- so the IPC JSON is
// what the dots trust. Urgent dots are not drawn: urgency is per window and
// the IPC list does not carry it.
Item {
    id: workspaces

    // The screen this bar instance sits on, handed in by the bar.
    property var barScreen: null
    readonly property string screenName: barScreen ? barScreen.name : ""

    property var entries: []
    property int activeId: -1

    // Old spec: the dot itself, and the button box around it -- 6px of padding
    // each side is what the hover tint filled.
    readonly property int dotSize: 6
    readonly property int activePillWidth: 20
    readonly property int buttonPad: 6

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
                    workspaces.entries = all
                        .filter(candidate => candidate.monitor === workspaces.screenName)
                        .sort((a, b) => a.id - b.id);
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
                readonly property bool hovered: dotArea.containsMouse

                // The holder tracks the pill, never the pointer: activation is
                // the one state allowed to reflow the row.
                width: (active ? workspaces.activePillWidth : workspaces.dotSize)
                    + workspaces.buttonPad * 2
                height: 24

                Behavior on width {
                    NumberAnimation {
                        duration: Theme.reduceMotion ? 0 : 180
                        easing.type: Easing.OutCubic
                    }
                }

                // The button-box hover tint, extending over the padding. Colour
                // only -- this is what hover is allowed to do.
                Rectangle {
                    x: 0
                    width: parent.width
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    radius: 8
                    color: Qt.alpha(Theme.text, 0.12)
                    visible: dotHolder.hovered && !dotHolder.active
                }

                // The dot, or the active pill: same element, same 6px height,
                // same 999-style stadium. 20px wide when active, 6 when not.
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    x: workspaces.buttonPad
                    width: dotHolder.active
                        ? workspaces.activePillWidth : workspaces.dotSize
                    height: workspaces.dotSize
                    radius: workspaces.dotSize / 2
                    color: dotHolder.active ? Qt.alpha(Theme.text, 0.95)
                        : dotHolder.hovered ? Qt.alpha(Theme.text, 0.65)
                        : Qt.alpha(Theme.text, 0.45)

                    Behavior on width {
                        NumberAnimation {
                            duration: Theme.reduceMotion ? 0 : 180
                            easing.type: Easing.OutCubic
                        }
                    }

                    Behavior on color {
                        ColorAnimation { duration: Theme.reduceMotion ? 0 : 180 }
                    }
                }

                // The hit target: constant, padded, independent of the visual.
                MouseArea {
                    id: dotArea

                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.LeftButton
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Hyprland.dispatch("workspace " + dotHolder.modelData.id)
                }
            }
        }
    }
}
