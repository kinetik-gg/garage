import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

PanelWindow {
    id: palette
    property bool grabReady: false
    property real panelHorizontalPadding: 28
    property real panelVerticalPadding: 14
    property real controlHorizontalPadding: 16
    property real controlVerticalPadding: 14

    signal modeSelected(string mode)
    signal dismissed()

    function focusedScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            const monitor = Hyprland.monitorFor(candidate);
            if (monitor && monitor.focused)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    function selectMode(mode) {
        palette.modeSelected(mode);
    }

    screen: focusedScreen()
    implicitWidth: content.implicitWidth + panelHorizontalPadding * 2
    implicitHeight: content.implicitHeight + panelVerticalPadding * 2
    color: "transparent"
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    anchors {
        bottom: true
    }

    margins.bottom: 48

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "garage-screenshot"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive

    Component.onCompleted: Qt.callLater(() => palette.grabReady = true)

    ContinuousRectangle {
        id: panel
        anchors.fill: parent
        color: Theme.contentTint
        // Matches the pill the compositor draws beneath it; the default 14px
        // radius left a rounded rectangle sitting on top of a pill.
        radius: height / 2
        power: 2

        RowLayout {
            id: content
            anchors.centerIn: parent
            spacing: 4

            ModeButton {
                foreground: Theme.text
                foregroundMuted: Theme.textMuted
                iconSource: "icons/monitor.svg"
                title: "Monitor"
                shortcut: "Space"
                horizontalPadding: palette.controlHorizontalPadding
                verticalPadding: palette.controlVerticalPadding
                onActivated: palette.selectMode("monitor")
            }

            ModeButton {
                foreground: Theme.text
                foregroundMuted: Theme.textMuted
                iconSource: "icons/app-window.svg"
                title: "Window"
                shortcut: "W"
                horizontalPadding: palette.controlHorizontalPadding
                verticalPadding: palette.controlVerticalPadding
                onActivated: palette.selectMode("window")
            }

            ModeButton {
                foreground: Theme.text
                foregroundMuted: Theme.textMuted
                iconSource: "icons/selection.svg"
                title: "Region"
                shortcut: "R"
                horizontalPadding: palette.controlHorizontalPadding
                verticalPadding: palette.controlVerticalPadding
                onActivated: palette.selectMode("region")
            }

            Rectangle {
                Layout.leftMargin: 8
                Layout.rightMargin: 8
                implicitWidth: 1
                Layout.fillHeight: true
                Layout.topMargin: palette.controlVerticalPadding / 2
                Layout.bottomMargin: palette.controlVerticalPadding / 2
                color: Theme.border
            }

            ContinuousRectangle {
                readonly property real controlPadding: palette.controlVerticalPadding
                implicitWidth: implicitHeight
                implicitHeight: closeIcon.height + controlPadding * 2
                radius: Theme.controlRadius
                color: closePointer.containsMouse ? Theme.hoverStrong : "transparent"

                Image {
                    id: closeIcon
                    anchors.centerIn: parent
                    width: 22
                    height: 22
                    source: "icons/x.svg"
                    sourceSize.width: 44
                    sourceSize.height: 44
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    antialiasing: true
                    mipmap: true
                    visible: false
                }

                // The svg carries its own colour, so it needs recolouring to
                // match the rest of the toolbar.
                ColorOverlay {
                    anchors.fill: closeIcon
                    source: closeIcon
                    color: Theme.text
                    cached: true
                }

                MouseArea {
                    id: closePointer
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: palette.dismissed()
                }
            }
        }
    }

    HyprlandFocusGrab {
        active: palette.grabReady
        windows: [palette]
        onCleared: {
            if (palette.grabReady)
                palette.dismissed();
        }
    }

    Shortcut {
        sequence: "Space"
        onActivated: palette.selectMode("monitor")
    }

    Shortcut {
        sequence: "W"
        onActivated: palette.selectMode("window")
    }

    Shortcut {
        sequence: "R"
        onActivated: palette.selectMode("region")
    }

    Shortcut {
        sequence: "Escape"
        onActivated: palette.dismissed()
    }
}
