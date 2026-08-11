import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

Scope {
    id: menu
    required property string targetScreenName
    // A launcher power result opens this component directly on the same
    // confirmation UI the Arch menu uses. Normal menu opens leave it empty.
    property string initialAction: ""
    property string pendingAction: menu.initialAction
    property int waybarBottom: 36
    property int reservedTop: 36

    signal actionSelected(string action)
    signal dismissed()

    // Dismissal goes through the motion so the exit has time to play; the shell
    // destroys this the moment dismissed() lands. Every path out of the menu
    // calls this rather than the signal.
    function requestDismissal() {
        motion.dismiss();
    }

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === menu.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    function choose(action, needsConfirmation) {
        if (!needsConfirmation) {
            menu.actionSelected(action);
            return;
        }

        menu.pendingAction = action;
    }

    function confirmationTitle() {
        const titles = {
            "reloadHyprland": "Reload Hyprland?",
            "suspend": "Put this system to sleep?",
            "restart": "Restart this system?",
            "poweroff": "Shut down this system?",
            "logout": "Log out " + Quickshell.env("USER") + "?",
            "lock": "Lock this system?"
        };
        return titles[menu.pendingAction] || "Continue?";
    }

    function confirmationAction() {
        const labels = {
            "reloadHyprland": "Reload",
            "suspend": "Sleep",
            "restart": "Restart",
            "poweroff": "Shut Down",
            "logout": "Log Out",
            "lock": "Lock"
        };
        return labels[menu.pendingAction] || "Continue";
    }

    function confirmationDescription() {
        const descriptions = {
            "reloadHyprland": "Hyprland will reload its configuration without closing your applications.",
            "suspend": "The system will sleep while keeping your desktop session available.",
            "restart": "The system will restart and all running applications will close.",
            "poweroff": "The system will shut down and all running applications will close.",
            "logout": "Your desktop session will end and all running applications will close.",
            "lock": "The lock screen will cover this session until you authenticate again."
        };
        return descriptions[menu.pendingAction] || "Open applications may contain unsaved work.";
    }

    function confirmationIcon() {
        const icons = {
            "reloadHyprland": "icons/arrows-clockwise.svg",
            "suspend": "icons/moon.svg",
            "restart": "icons/arrow-counter-clockwise.svg",
            "poweroff": "icons/power.svg",
            "logout": "icons/sign-out.svg",
            "lock": "icons/lock-simple.svg"
        };
        return icons[menu.pendingAction] || "icons/power.svg";
    }

    function confirmPending() {
        if (menu.pendingAction !== "")
            menu.actionSelected(menu.pendingAction);
    }

    function cancelPending() {
        menu.pendingAction = "";
        menu.requestDismissal();
    }

    function updateWaybarGeometry(text) {
        try {
            const monitor = Hyprland.monitorFor(menu.targetScreen());
            const output = monitor ? JSON.parse(text)[monitor.name] : null;
            if (!output || !output.levels)
                return;

            const monitorY = monitor.lastIpcObject && monitor.lastIpcObject.y
                ? monitor.lastIpcObject.y : 0;
            const reserved = monitor.lastIpcObject
                ? monitor.lastIpcObject.reserved : null;
            if (reserved && reserved.length > 1)
                menu.reservedTop = reserved[1];
            let bottom = 0;
            for (const level of Object.values(output.levels)) {
                for (const layer of level) {
                    if (layer.namespace === "waybar")
                        bottom = Math.max(bottom, layer.y - monitorY + layer.h);
                }
            }

            if (bottom > 0)
                menu.waybarBottom = bottom;
        } catch (error) {
            console.warn("Unable to resolve Waybar geometry:", error);
        }
    }

    Process {
        command: ["hyprctl", "layers", "-j"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: menu.updateWaybarGeometry(text)
        }
    }

    PanelWindow {
        id: palette
        visible: menu.pendingAction === ""
        screen: menu.targetScreen()
        implicitWidth: 292
        implicitHeight: 284
        color: "transparent"
        focusable: true
        aboveWindows: true
        exclusiveZone: 0
        surfaceFormat.opaque: false

        // Top-to-bottom entrance, shared with every other palette. See
        // PanelMotion. The menu drops out of the logo it was opened from, the
        // same way the bar's panels drop out of their own modules.
        PanelMotion {
            id: motion
            onFinished: menu.dismissed()
        }

        anchors {
            left: true
            top: true
        }

        margins.left: 6
        // Overlay surfaces already begin below Waybar's exclusive zone.
        margins.top: motion.surfaceTop

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "garage-session-menu"
        WlrLayershell.keyboardFocus: menu.pendingAction === ""
            ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

        Rectangle {
            anchors.fill: parent
            color: Theme.contentTint
            opacity: motion.opacity

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 6
                spacing: 0

                SessionMenuItem {
                    title: "About This System"
                    shortcut: "SUPER ⇧ A"
                    onActivated: menu.choose("about", false)
                }

                SessionMenuItem {
                    title: "System Preferences…"
                    shortcut: "SUPER ⇧ ,"
                    onActivated: menu.choose("preferences", false)
                }

                SessionMenuItem {
                    title: "Reload Hyprland…"
                    onActivated: menu.choose("reloadHyprland", true)
                }

                MenuSeparator {}

                SessionMenuItem {
                    title: "Sleep…"
                    onActivated: menu.choose("suspend", true)
                }

                SessionMenuItem {
                    title: "Restart…"
                    onActivated: menu.choose("restart", true)
                }

                SessionMenuItem {
                    title: "Shut Down…"
                    onActivated: menu.choose("poweroff", true)
                }

                MenuSeparator {}

                SessionMenuItem {
                    title: "Lock Screen"
                    shortcut: "SUPER + L"
                    onActivated: menu.choose("lock", false)
                }

                SessionMenuItem {
                    title: "Log Out " + Quickshell.env("USER") + "…"
                    onActivated: menu.choose("logout", true)
                }
            }

        }
    }

    PanelWindow {
        id: confirmationBackdrop
        readonly property var hyprMonitor: Hyprland.monitorFor(menu.targetScreen())
        readonly property var reserved: hyprMonitor && hyprMonitor.lastIpcObject
            ? hyprMonitor.lastIpcObject.reserved : null

        visible: menu.pendingAction !== ""
        screen: menu.targetScreen()
        color: "transparent"
        focusable: false
        aboveWindows: true
        exclusionMode: ExclusionMode.Ignore
        surfaceFormat.opaque: false

        anchors {
            left: true
            right: true
            top: true
            bottom: true
        }

        margins.left: reserved && reserved.length > 0 ? -reserved[0] : 0
        margins.top: reserved && reserved.length > 1 ? -reserved[1] : 0
        margins.right: reserved && reserved.length > 2 ? -reserved[2] : 0
        margins.bottom: reserved && reserved.length > 3 ? -reserved[3] : 0

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "garage-session-confirmation-dismiss"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

        MouseArea {
            anchors.fill: parent
            onClicked: menu.cancelPending()
        }
    }

    PanelWindow {
        id: confirmationWindow
        visible: menu.pendingAction !== ""
        screen: menu.targetScreen()
        implicitWidth: 480
        implicitHeight: 200
        color: "transparent"
        focusable: true
        aboveWindows: true
        exclusionMode: ExclusionMode.Ignore
        surfaceFormat.opaque: false

        anchors {
            left: true
            top: true
        }

        margins.left: Math.round((screen.width - implicitWidth) / 2)
        margins.top: Math.round((screen.height - menu.reservedTop - implicitHeight) / 2)

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "garage-session-confirmation"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive

        Shortcut {
            sequence: "Escape"
            onActivated: menu.cancelPending()
        }

        Rectangle {
            id: confirmationDialog
            anchors.fill: parent
            color: Theme.contentTint

            MouseArea {
                anchors.fill: parent
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.leftMargin: 30
                anchors.rightMargin: 30
                anchors.topMargin: 20
                anchors.bottomMargin: 20
                spacing: 16

                RowLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 18

                    ContinuousRectangle {
                        Layout.preferredWidth: 76
                        Layout.preferredHeight: 76
                        Layout.alignment: Qt.AlignVCenter
                        // A decorative well rather than a control, so it takes
                        // the window corner and reads like the dialog around it.
                        radius: Theme.cornerRadius
                        color: Theme.iconWell

                        Image {
                            id: confirmationGlyph
                            anchors.centerIn: parent
                            width: 42
                            height: 42
                            source: menu.confirmationIcon()
                            sourceSize.width: 84
                            sourceSize.height: 84
                            fillMode: Image.PreserveAspectFit
                            smooth: true
                            antialiasing: true
                            mipmap: true
                            visible: false
                        }

                        // The svg ships its own colour, so it has to be
                        // recoloured to read against the darker well.
                        ColorOverlay {
                            anchors.fill: confirmationGlyph
                            source: confirmationGlyph
                            color: Theme.iconWellGlyph
                            cached: true
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        spacing: 8

                        Text {
                            Layout.fillWidth: true
                            text: menu.confirmationTitle()
                            color: Theme.text
                            font.family: Theme.sans
                            font.pixelSize: 16
                            font.weight: Font.Bold
                            wrapMode: Text.WordWrap
                            renderType: Text.NativeRendering
                        }

                        Text {
                            Layout.fillWidth: true
                            text: menu.confirmationDescription()
                            color: Theme.textMuted
                            font.family: Theme.sans
                            font.pixelSize: 12
                            lineHeight: 1.15
                            wrapMode: Text.WordWrap
                            renderType: Text.NativeRendering
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 32
                    spacing: 8

                    Item { Layout.fillWidth: true }

                    ContinuousRectangle {
                        implicitWidth: 88
                        implicitHeight: 32
                        radius: Theme.controlRadius
                        // Ghost: the confirming action is the prominent one, so
                        // an outline here only competes with it.
                        color: cancelPointer.containsMouse ? Theme.hoverStrong : "transparent"

                        Text {
                            anchors.centerIn: parent
                            text: "Cancel"
                            color: Theme.text
                            font.family: Theme.sans
                            font.pixelSize: 12
                        }

                        MouseArea {
                            id: cancelPointer
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: menu.cancelPending()
                        }
                    }

                    ContinuousRectangle {
                        implicitWidth: 104
                        implicitHeight: 32
                        radius: Theme.controlRadius
                        // The accent fill already separates it from the dialog,
                        // so an outline only muddies the edge.
                        color: confirmPointer.containsMouse ? Theme.accentHover : Theme.accent

                        Text {
                            anchors.centerIn: parent
                            text: menu.confirmationAction()
                            color: Theme.accentText
                            font.family: Theme.sans
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                        }

                        MouseArea {
                            id: confirmPointer
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: menu.confirmPending()
                        }
                    }
                }
            }
        }
    }

    Shortcut { sequence: "A"; enabled: menu.pendingAction === ""; onActivated: menu.choose("about", false) }
    Shortcut { sequence: "F"; enabled: menu.pendingAction === ""; onActivated: menu.choose("reloadHyprland", true) }
    Shortcut { sequence: "L"; enabled: menu.pendingAction === ""; onActivated: menu.choose("lock", false) }
    Shortcut { sequence: "O"; enabled: menu.pendingAction === ""; onActivated: menu.choose("logout", true) }
    Shortcut { sequence: "S"; enabled: menu.pendingAction === ""; onActivated: menu.choose("suspend", true) }
    Shortcut { sequence: "R"; enabled: menu.pendingAction === ""; onActivated: menu.choose("restart", true) }
    Shortcut { sequence: "P"; enabled: menu.pendingAction === ""; onActivated: menu.choose("poweroff", true) }
    Shortcut { sequence: "Return"; enabled: menu.pendingAction !== ""; onActivated: menu.confirmPending() }
    Shortcut {
        sequence: "Escape"
        onActivated: {
            if (menu.pendingAction !== "")
                menu.cancelPending();
            else
                menu.requestDismissal();
        }
    }
}
