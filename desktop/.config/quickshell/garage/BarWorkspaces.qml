import Quickshell.Hyprland
import QtQuick

// The workspace indicator: one dot per workspace on this output, sorted by id,
// widening when active and reddening when a window on it is urgent. Clicking a
// dot activates its workspace -- the ext/workspaces module's own behaviour.
//
// The dots read Hyprland's global workspace model and filter it by monitor name,
// rather than asking a per-monitor model that only fills once the compositor's
// event stream has answered for that specific output. The global model is the
// reactive one: every workspace event lands in it whether or not any per-screen
// bookkeeping has run yet.
Item {
    id: workspaces

    // The screen this bar instance sits on, handed in by the bar.
    property var barScreen: null
    readonly property string screenName: barScreen ? barScreen.name : ""

    readonly property var entries: {
        const list = Hyprland.workspaces ? Hyprland.workspaces.values : [];
        const mine = list.filter(candidate =>
            candidate.monitor && candidate.monitor.name === screenName);
        return mine.sort((a, b) => a.id - b.id);
    }

    implicitWidth: dotRow.implicitWidth
    implicitHeight: 24
    visible: entries.length > 0

    Row {
        id: dotRow

        anchors.centerIn: parent
        spacing: BarState.scaled("workspaceGap")

        Repeater {
            model: workspaces.entries

            delegate: Item {
                id: dotHolder

                required property var modelData

                readonly property bool active: modelData.focused === true
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
