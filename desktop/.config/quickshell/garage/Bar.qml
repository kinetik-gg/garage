import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import QtQuick

// The Garage bar: the waybar replacement, in-process.
//
// One PanelWindow per screen, top layer, owning an exclusive zone of exactly the
// configured height -- which is what lets every overlay surface keep its existing
// below-the-bar margins without knowing this file exists. Layout mirrors the old
// fragments: menu, workspaces and media on the left; metric strips, the AI usage
// chip, the context chips, the tray and the icon trio with the clock on the right.
//
// Clicks do not cross a process boundary. A module that opens a surface emits it
// here with this screen's name and the module's monitor-local anchor X, and the
// shell -- which instantiates this component -- routes it through the same
// functions the keybinds have always used.
Scope {
    id: bar

    // surface names: session | launcher | notifications | controlCenter | media |
    // monitor:<widget>. anchorX of -1 means centred, as a keybind would open it.
    signal surfaceRequested(string surface, string screenName, real anchorX)

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: output

            required property var modelData
            readonly property string screenName: modelData.name

            screen: modelData
            color: "transparent"
            aboveWindows: true
            // Explicit, marker-driven height. Left to its own devices the window
            // sizes from its contents' implicit measurement, which races the first
            // layout and lands on a comical ~100px default; the palettes set their
            // own implicit sizes for the same reason.
            implicitHeight: BarState.height
            exclusiveZone: BarState.height
            focusable: false
            surfaceFormat.opaque: false
            anchors {
                top: true
                left: true
                right: true
            }

            WlrLayershell.layer: WlrLayer.Top
            WlrLayershell.namespace: "garage-bar"

            // The tint. "blurred" is the translucent body over Hyprland's layer blur
            // -- the same 42% alpha the stylesheet carried -- and "transparent" is
            // that body at zero alpha, leaving only the blur behind the bar.
            Rectangle {
                anchors.fill: parent
                color: BarState.background === "transparent"
                    ? "transparent"
                    : Qt.rgba(Theme.bodyBase.r, Theme.bodyBase.g, Theme.bodyBase.b, 0.42)
            }

            function centreX(item) {
                if (!item)
                    return -1;
                return item.mapToItem(contentRow, item.width / 2, 0).x;
            }

            Item {
                id: contentRow

                anchors.fill: parent
                anchors.leftMargin: BarState.scaled("edge")
                anchors.rightMargin: BarState.scaled("edge")

                Row {
                    id: left

                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: BarState.scaled("menuRight")

                    BarIconButton {
                        glyph: "\uf303"
                        glyphFamily: "Caskaydia Mono Nerd Font Mono"
                        glyphSize: 17
                        onActivated: bar.surfaceRequested(
                            "session", output.screenName, -1)
                    }

                    BarWorkspaces {
                        visible: BarState.indicator
                        barScreen: output.modelData
                    }

                    BarMediaChip {
                        id: mediaChip

                        visible: BarState.mediaPlayer && MediaController.visible
                        onActivated: bar.surfaceRequested(
                            "media", output.screenName, output.centreX(mediaChip))
                    }
                }

                Row {
                    id: right

                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: BarState.scaled("module")

                    // The metric strips, in BAR_METRICS order, each gated by its own
                    // switch and fed by the one shared stream.
                    Repeater {
                        id: metricRepeater

                        model: [
                            { key: "cpu", enabled: true },
                            { key: "memory", enabled: true },
                            { key: "network", enabled: false },
                            { key: "temp", enabled: false },
                            { key: "disk", enabled: false },
                            { key: "gpu", enabled: false }
                        ]

                        delegate: BarMetricStrip {
                            id: stripItem

                            required property var modelData

                            visible: BarState.monitors[modelData.key] === true
                            name: modelData.key
                            series: MetricsService.seriesFor(modelData.key)
                            value: MetricsService.labelFor(modelData.key)
                            tip: MetricsService.tipFor(modelData.key)
                            onActivated: bar.surfaceRequested(
                                "monitor:" + modelData.key,
                                output.screenName, output.centreX(stripItem))
                        }
                    }

                    BarChip {
                        id: aiChip

                        visible: BarState.aiUsage && BarContext.aiGlyph !== ""
                        label: BarContext.aiGlyph
                        labelFont: "Phosphor"
                        warning: BarContext.aiStale
                        tip: BarContext.aiTip
                        onActivated: bar.surfaceRequested(
                            "aiUsage", output.screenName, -1)
                    }

                    BarChip {
                        visible: BarContext.containersAvailable
                            && BarContext.containerCount > 0
                        label: "CTR " + BarContext.containerCount
                        tip: "Running containers\n"
                            + BarContext.containerNames.join("\n")
                    }

                    BarChip {
                        visible: BarContext.smbAvailable
                        label: "SMB " + BarContext.smbConnected
                        warning: BarContext.smbConnected < BarContext.smbExpected
                        tip: BarContext.smbConnected === BarContext.smbExpected
                            ? "All " + BarContext.smbExpected + " SMB shares connected"
                            : "Connected " + BarContext.smbConnected + " / "
                                + BarContext.smbExpected + "\nUnavailable\n"
                                + BarContext.smbMissingLabels.join("\n")
                    }

                    // The mic dot: dim when idle, accent when something records.
                    Item {
                        width: 10
                        height: 10
                        visible: BarContext.micAvailable

                        Rectangle {
                            anchors.centerIn: parent
                            width: 8
                            height: 8
                            radius: 4
                            color: BarContext.micRecording ? Theme.accent
                                : Qt.alpha(Theme.text, 0.45)

                            Behavior on color {
                                ColorAnimation {
                                    duration: Theme.reduceMotion ? 0 : 180
                                }
                            }
                        }
                    }

                    BarTray {}

                    BarIconButton {
                        glyph: "\ue0ce"
                        onActivated: bar.surfaceRequested(
                            "notifications", output.screenName, -1)
                    }

                    BarIconButton {
                        glyph: "\ue30c"
                        onActivated: bar.surfaceRequested(
                            "launcher", output.screenName, -1)
                    }

                    BarIconButton {
                        glyph: "\ue676"
                        onActivated: bar.surfaceRequested(
                            "controlCenter", output.screenName, -1)
                    }

                    BarClock {}
                }
            }
        }
    }
}
