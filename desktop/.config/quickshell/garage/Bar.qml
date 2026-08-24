import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import QtQuick

// One edge-aware PanelWindow per output, composed from three ordered extension
// rails. The marker owns order and docking; the registry owns which ids exist.
Scope {
    id: bar

    signal surfaceRequested(string surface, string screenName, real anchor)

    readonly property var services: ({
        barState: BarState,
        context: BarContext,
        metrics: MetricsService,
        media: MediaController,
        workspaces: WorkspaceService,
        theme: Theme,
        paths: GaragePaths
    })

    Process { id: positionProcess }

    function setPosition(edge) {
        if (!BarState.validPosition(edge) || positionProcess.running)
            return;
        positionProcess.command = [GaragePaths.garage, "set", "bar.position",
            JSON.stringify(edge)];
        positionProcess.running = true;
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: output
            required property var modelData
            readonly property string screenName: modelData.name
            readonly property bool vertical: BarState.vertical

            screen: modelData
            color: "transparent"
            aboveWindows: true
            implicitWidth: vertical ? BarState.thickness : 0
            implicitHeight: vertical ? 0 : BarState.thickness
            exclusiveZone: BarState.thickness
            focusable: false
            surfaceFormat.opaque: false

            anchors.top: BarState.position !== "bottom"
            anchors.bottom: BarState.position === "bottom"
                || BarState.position === "left" || BarState.position === "right"
            anchors.left: BarState.position !== "right"
            anchors.right: BarState.position === "right"
                || BarState.position === "top" || BarState.position === "bottom"

            WlrLayershell.layer: WlrLayer.Top
            WlrLayershell.namespace: "garage-bar"

            Rectangle {
                anchors.fill: parent
                color: BarState.background === "transparent" ? "transparent"
                    : Qt.rgba(Theme.bodyBase.r, Theme.bodyBase.g,
                        Theme.bodyBase.b, 0.42)
            }

            Item {
                id: content
                z: 1
                anchors.fill: parent
                anchors.leftMargin: output.vertical ? 0 : BarState.scaled("edge")
                anchors.rightMargin: output.vertical ? 0 : BarState.scaled("edge")
                anchors.topMargin: output.vertical ? BarState.scaled("edge") : 0
                anchors.bottomMargin: output.vertical ? BarState.scaled("edge") : 0

                BarRail {
                    id: startRail
                    registry: ExtensionRegistry
                    services: bar.services
                    screen: output.modelData
                    screenName: output.screenName
                    railRole: "left"
                    edge: BarState.position
                    extensionIds: BarState.left
                    anchors.left: output.vertical ? undefined : parent.left
                    anchors.top: output.vertical ? parent.top : undefined
                    anchors.verticalCenter: output.vertical ? undefined : parent.verticalCenter
                    anchors.horizontalCenter: output.vertical ? parent.horizontalCenter : undefined
                    onSurfaceRequested: (surface, name, anchor) =>
                        bar.surfaceRequested(surface, name, anchor)
                }

                BarRail {
                    id: centerRail
                    registry: ExtensionRegistry
                    services: bar.services
                    screen: output.modelData
                    screenName: output.screenName
                    railRole: "center"
                    edge: BarState.position
                    extensionIds: BarState.center
                    x: output.vertical ? Math.round((parent.width - width) / 2)
                        : Math.max(startRail.x + startRail.width
                            + BarState.scaled("module"),
                            Math.min(Math.round((parent.width - width) / 2),
                                endRail.x - width - BarState.scaled("module")))
                    y: output.vertical
                        ? Math.max(startRail.y + startRail.height
                            + BarState.scaled("module"),
                            Math.min(Math.round((parent.height - height) / 2),
                                endRail.y - height - BarState.scaled("module")))
                        : Math.round((parent.height - height) / 2)
                    onSurfaceRequested: (surface, name, anchor) =>
                        bar.surfaceRequested(surface, name, anchor)
                }

                BarRail {
                    id: endRail
                    registry: ExtensionRegistry
                    services: bar.services
                    screen: output.modelData
                    screenName: output.screenName
                    railRole: "right"
                    edge: BarState.position
                    extensionIds: BarState.right
                    anchors.right: output.vertical ? undefined : parent.right
                    anchors.bottom: output.vertical ? parent.bottom : undefined
                    anchors.verticalCenter: output.vertical ? undefined : parent.verticalCenter
                    anchors.horizontalCenter: output.vertical ? parent.horizontalCenter : undefined
                    onSurfaceRequested: (surface, name, anchor) =>
                        bar.surfaceRequested(surface, name, anchor)
                }
            }

            BarDragOverlay {
                barScreen: output.modelData
                edge: BarState.position
                onEdgeDropped: edge => bar.setPosition(edge)
            }
        }
    }
}
