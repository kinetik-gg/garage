import Quickshell
import Quickshell.Wayland
import QtQuick

// Edge-aware layer-surface host used by extension popovers. targetAnchor is a
// coordinate on the bar's long axis; a negative value centres the surface.
PanelWindow {
    id: surface

    property var targetScreen: null
    property string targetScreenName: ""
    property string edge: BarState.position
    property real targetAnchor: -1
    property int gutter: Theme.windowGutter
    // Distance from the opening edge. Most palettes use the same value as the
    // corner gutter; monitor-relative surfaces can choose their own offset
    // without changing the long-axis centring/clamp.
    property int surfaceOffset: gutter
    property string surfaceNamespace: "garage-palette"
    property int keyboardFocusMode: WlrKeyboardFocus.OnDemand
    property bool escapeEnabled: true
    property bool dismissing: false
    readonly property var effectiveScreen: targetScreen || resolveTargetScreen()
    readonly property real contentOpacity: motion.opacity
    readonly property real animatedMargin: motion.surfaceMargin

    signal dismissed()
    signal motionFinished()

    screen: effectiveScreen
    color: "transparent"
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    anchors.top: edge === "top" || edge === "left" || edge === "right"
    anchors.bottom: edge === "bottom"
    anchors.left: edge === "left" || edge === "top" || edge === "bottom"
    anchors.right: edge === "right"

    margins.top: edge === "top" ? Math.round(motion.surfaceMargin)
        : (edge === "left" || edge === "right" ? longAxisMargin() : 0)
    margins.bottom: edge === "bottom" ? Math.round(motion.surfaceMargin) : 0
    margins.left: edge === "left" ? Math.round(motion.surfaceMargin)
        : (edge === "top" || edge === "bottom" ? longAxisMargin() : 0)
    margins.right: edge === "right" ? Math.round(motion.surfaceMargin) : 0

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: surfaceNamespace
    WlrLayershell.keyboardFocus: keyboardFocusMode

    function resolveTargetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    function longAxisMargin() {
        const target = effectiveScreen;
        const span = target ? (edge === "top" || edge === "bottom"
            ? target.width : target.height) : 0;
        const extent = edge === "top" || edge === "bottom"
            ? implicitWidth : implicitHeight;
        const wanted = targetAnchor < 0 ? (span - extent) / 2
            : targetAnchor - extent / 2;
        return Math.round(Math.max(gutter, Math.min(wanted,
            span - extent - gutter)));
    }

    function dismissSurface() {
        if (!dismissing) {
            dismissing = true;
            motion.dismiss();
        }
    }

    PanelMotion {
        id: motion
        edge: surface.edge
        restingMargin: surface.surfaceOffset
        onFinished: {
            surface.motionFinished();
            surface.dismissed();
        }
    }

    Shortcut {
        sequence: "Escape"
        enabled: surface.escapeEnabled
        onActivated: surface.dismissSurface()
    }
}
