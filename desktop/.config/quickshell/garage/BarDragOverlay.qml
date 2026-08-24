import Quickshell
import Quickshell.Wayland
import QtQuick

// The handler sits behind the rails, so only the bar background starts a drag.
// Once grabbed it maps a click-through fullscreen guide with four drop zones;
// the actual bar moves only after garage publishes the new marker.
Item {
    id: overlay
    required property var barScreen
    property string edge: BarState.position
    property string candidateEdge: edge

    signal edgeDropped(string edge)

    anchors.fill: parent
    z: 0

    function screenPoint(scenePoint) {
        // DragHandler reports coordinates in the narrow bar surface. Rebase
        // the two far-edge surfaces so edge distances are monitor-local.
        return Qt.point(
            scenePoint.x + (edge === "right"
                ? barScreen.width - BarState.thickness : 0),
            scenePoint.y + (edge === "bottom"
                ? barScreen.height - BarState.thickness : 0));
    }

    function nearestEdge(point) {
        const distances = {
            top: point.y,
            bottom: barScreen.height - point.y,
            left: point.x,
            right: barScreen.width - point.x
        };
        let nearest = "top";
        for (const name of ["bottom", "left", "right"]) {
            if (distances[name] < distances[nearest])
                nearest = name;
        }
        return nearest;
    }

    DragHandler {
        id: drag
        target: null
        acceptedButtons: Qt.LeftButton
        onCentroidChanged: {
            if (active)
                overlay.candidateEdge = overlay.nearestEdge(
                    overlay.screenPoint(centroid.scenePosition));
        }
        onActiveChanged: {
            if (!active && overlay.candidateEdge !== overlay.edge)
                overlay.edgeDropped(overlay.candidateEdge);
            if (!active)
                overlay.candidateEdge = overlay.edge;
        }
    }

    PanelWindow {
        id: guide
        visible: drag.active
        screen: overlay.barScreen
        color: "transparent"
        focusable: false
        aboveWindows: true
        exclusionMode: ExclusionMode.Ignore
        surfaceFormat.opaque: false
        anchors.top: true
        anchors.bottom: true
        anchors.left: true
        anchors.right: true
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "garage-bar-dock-guide"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        mask: Region {}

        component DropZone: Rectangle {
            required property string zone
            color: overlay.candidateEdge === zone
                ? Qt.alpha(Theme.accent, 0.28) : Qt.alpha(Theme.bodyBase, 0.12)
            border.color: overlay.candidateEdge === zone
                ? Theme.accent : Theme.frameOuter
            border.width: 1
        }

        DropZone {
            zone: "top"
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: BarState.thickness
        }
        DropZone {
            zone: "bottom"
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            height: BarState.thickness
        }
        DropZone {
            zone: "left"
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            width: BarState.thickness
        }
        DropZone {
            zone: "right"
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: parent.right
            width: BarState.thickness
        }
    }
}
