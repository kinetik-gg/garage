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

        Scope {
            id: outputScope
            required property var modelData
            property string loadedEdge: BarState.position

            // Layer-shell anchor and orientation changes are not reliably
            // atomic. Recreate the output window after an edge change so the
            // compositor receives a fresh buffer with its final dimensions.
            LazyLoader {
                id: outputLoader
                active: true

                PanelWindow {
                    id: output
                    readonly property string edge: outputScope.loadedEdge
                    readonly property string screenName: outputScope.modelData.name
                    readonly property bool vertical: edge === "left" || edge === "right"

                    screen: outputScope.modelData
                    color: "transparent"
                    aboveWindows: true
                    implicitWidth: vertical ? BarState.thickness : 0
                    implicitHeight: vertical ? 0 : BarState.thickness
                    exclusiveZone: BarState.thickness
                    focusable: false
                    surfaceFormat.opaque: false

                    anchors.top: output.edge !== "bottom"
                    anchors.bottom: output.edge === "bottom"
                        || output.edge === "left" || output.edge === "right"
                    anchors.left: output.edge !== "right"
                    anchors.right: output.edge === "right"
                        || output.edge === "top" || output.edge === "bottom"

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
                            screen: outputScope.modelData
                            screenName: output.screenName
                            railRole: "left"
                            edge: output.edge
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
                            screen: outputScope.modelData
                            screenName: output.screenName
                            railRole: "center"
                            edge: output.edge
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
                            screen: outputScope.modelData
                            screenName: output.screenName
                            railRole: "right"
                            edge: output.edge
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
                        barScreen: outputScope.modelData
                        edge: output.edge
                        onEdgeDropped: edge => bar.setPosition(edge)
                    }
                }
            }

            Timer {
                id: recreateTimer
                interval: 1
                onTriggered: outputLoader.active = true
            }

            Connections {
                target: BarState
                function onPositionChanged() {
                    outputLoader.active = false;
                    outputScope.loadedEdge = BarState.position;
                    recreateTimer.restart();
                }
            }
        }
    }
}
