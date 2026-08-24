import Quickshell
import QtQuick

// The workspace indicator, drawn to the old indicator's own spec.
//
// Geometry comes from the bar's typed spacing table: an idle dot is 6x6 at 45%
// of the foreground, hover
// raises the dot to 65% and paints a soft 8px-radius tint behind the button,
// and the ACTIVE workspace is a 20x6 stadium at 95% -- a pill, not a dot. The
// pill's width animates over 180ms; that is the only width in the row that
// ever changes, because a width that answers to hover shoves every dot to its
// right around -- the layout shift this indicator is forbidden to have. Hover
// is colour-only.
//
// Data comes from WorkspaceService, which reads `hyprctl -j workspaces` and
// `-j monitors` once for the whole shell and projects the result by output.
// Quickshell's own Hyprland
// workspace model is a placeholder on this build -- every entry answers
// id=-1, monitor=null even after an explicit refresh -- so the IPC JSON is
// what the dots trust. Urgent dots are not drawn: urgency is per window and
// the IPC list does not carry it.
Item {
    id: workspaces

    // The screen this bar instance sits on, handed in by the bar.
    property var barScreen: null
    property var workspaceService: WorkspaceService
    property bool vertical: false
    readonly property string screenName: barScreen ? barScreen.name : ""

    readonly property int workspaceRevision: workspaceService.revision
    readonly property var entries: {
        const currentRevision = workspaces.workspaceRevision;
        return workspaces.workspaceService.entriesFor(workspaces.screenName);
    }
    readonly property int activeId: {
        const currentRevision = workspaces.workspaceRevision;
        return workspaces.workspaceService.activeFor(workspaces.screenName);
    }

    // Old spec: the dot itself, and the button box around it -- 6px of padding
    // each side is what the hover tint filled.
    readonly property int dotSize: 6
    readonly property int activePillWidth: 20
    readonly property int buttonPad: 6

    implicitWidth: vertical ? verticalDots.implicitWidth
        : horizontalDots.implicitWidth
    implicitHeight: vertical ? verticalDots.implicitHeight
        : horizontalDots.implicitHeight
    visible: entries.length > 0

    Component.onCompleted: workspaceService.refresh()

    // Switching: `hyprctl dispatch hl.dsp.focus({ workspace = N })`. Two
    // dead ends sit behind that spelling, both probed on this compositor:
    // Quickshell's own Hyprland.dispatch("workspace N") returns silently
    // without switching, and the classic raw form dies in Hyprland 0.56+'s
    // Lua dispatch layer -- hyprctl wraps its argument in
    // `return hl.dispatch( ... )`, where `workspace 21` is a syntax error.
    // The focus-table form is the one the layer answers `ok` to, and the
    // one the generated hypridle config already uses for dpms.
    function activate(id) {
        workspaceService.activate(id);
    }

    component WorkspaceDot: Item {
        id: dotHolder

        required property var modelData

        readonly property bool active: workspaces.activeId === modelData.id
        readonly property bool hovered: dotArea.containsMouse

        // The holder tracks the pill, never the pointer: activation is the one
        // state allowed to reflow the positioner.
        width: workspaces.vertical ? 24
            : (active ? workspaces.activePillWidth : workspaces.dotSize)
                + workspaces.buttonPad * 2
        height: workspaces.vertical
            ? (active ? workspaces.activePillWidth : workspaces.dotSize)
                + workspaces.buttonPad * 2
            : 24

        Behavior on width {
            NumberAnimation {
                duration: Theme.reduceMotion ? 0 : 180
                easing.type: Easing.OutCubic
            }
        }

        Behavior on height {
            NumberAnimation {
                duration: Theme.reduceMotion ? 0 : 180
                easing.type: Easing.OutCubic
            }
        }

        Rectangle {
            anchors.fill: parent
            radius: 8
            color: Qt.alpha(Theme.text, 0.12)
            visible: dotHolder.hovered && !dotHolder.active
        }

        Rectangle {
            anchors.centerIn: parent
            width: workspaces.vertical ? workspaces.dotSize
                : (dotHolder.active ? workspaces.activePillWidth
                    : workspaces.dotSize)
            height: workspaces.vertical
                ? (dotHolder.active ? workspaces.activePillWidth
                    : workspaces.dotSize) : workspaces.dotSize
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

            Behavior on height {
                NumberAnimation {
                    duration: Theme.reduceMotion ? 0 : 180
                    easing.type: Easing.OutCubic
                }
            }

            Behavior on color {
                ColorAnimation { duration: Theme.reduceMotion ? 0 : 180 }
            }
        }

        MouseArea {
            id: dotArea

            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton
            cursorShape: Qt.PointingHandCursor
            onClicked: workspaces.activate(dotHolder.modelData.id)
        }
    }

    Row {
        id: horizontalDots
        anchors.centerIn: parent
        spacing: BarState.scaled("workspaceGap")

        Repeater {
            model: workspaces.vertical ? [] : workspaces.entries
            delegate: WorkspaceDot {}
        }
    }

    Column {
        id: verticalDots
        anchors.centerIn: parent
        spacing: BarState.scaled("workspaceGap")

        Repeater {
            model: workspaces.vertical ? workspaces.entries : []
            delegate: WorkspaceDot {}
        }
    }
}
