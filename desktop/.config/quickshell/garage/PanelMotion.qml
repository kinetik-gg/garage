import QtQuick

// The entrance and exit every palette in the shell shares.
//
// A layer surface is exactly the size of the panel it carries, so a transform
// inside it travels through a stationary rectangle and reads as the contents
// scrolling behind a fixed frame. What moves is the surface's own top margin,
// which the compositor honours by repositioning the whole thing -- Glass, frame
// and contents together -- on every frame.
//
// Client-side rather than a Hyprland layer rule because the shape is not one a
// layer rule can describe: an overshoot on the way in, a fade with its own much
// shorter timing on the way out, and Reduce Motion honoured for both. The
// palettes' namespaces are all no_anim in windowrules.lua so nothing animates
// the same surface twice.
//
// The travel follows the edge the opener belongs to. Existing callers only read
// surfaceTop and therefore keep their top-edge behavior; PaletteSurface reads
// surfaceMargin and applies it to the selected edge.
//
// Usage, in a PanelWindow:
//
//     PanelMotion { id: motion; onFinished: palette.dismissed() }
//     margins.top: motion.surfaceTop
//     ...
//     opacity: motion.opacity
//
// and the palette's requestDismissal() calls motion.dismiss().
QtObject {
    id: motion

    // Where the panel comes to rest, and how far above that it starts. The
    // travel is short on purpose: it gives the entrance a direction, it is not
    // there to make anyone watch a panel cross the screen.
    property string edge: "top"
    property real restingMargin: Theme.windowGutter
    // Compatibility for the existing palettes while they migrate to
    // PaletteSurface.
    property alias restingTop: motion.restingMargin
    readonly property real liftDistance: 34

    property bool revealed: false
    property bool closing: false
    readonly property bool presented: motion.revealed && !motion.closing

    // Raised once the exit has finished and the surface can be destroyed.
    signal finished()

    // Bound to `revealed`, not to `presented`, so the exit does not move it.
    // There is nothing a client can fade about a layer surface except its own
    // contents: the compositor keeps blurring the surface until it is unmapped,
    // so a panel that travels on the way out drags an empty blurred rectangle up
    // the screen behind it. The exit is a fade and nothing else.
    property real surfaceMargin: motion.revealed
        ? motion.restingMargin : motion.restingMargin - motion.liftDistance
    readonly property real surfaceTop: motion.edge === "top"
        ? motion.surfaceMargin : motion.restingMargin
    readonly property real surfaceBottom: motion.edge === "bottom"
        ? motion.surfaceMargin : motion.restingMargin
    readonly property real surfaceLeft: motion.edge === "left"
        ? motion.surfaceMargin : motion.restingMargin
    readonly property real surfaceRight: motion.edge === "right"
        ? motion.surfaceMargin : motion.restingMargin

    property real opacity: motion.presented ? 1 : 0

    // OutBack is the whole point: it carries the panel a few pixels past where
    // it belongs and lets it settle back. The overshoot is turned well down from
    // Qt's 1.70158 default, which at this travel is a bounce rather than the
    // suggestion of weight this wants.
    Behavior on surfaceMargin {
        NumberAnimation {
            duration: Theme.reduceMotion ? 0 : 340
            easing.type: Easing.OutBack
            easing.overshoot: 1.1
        }
    }

    Behavior on opacity {
        NumberAnimation {
            // Short on the way out. Every millisecond between the contents going
            // and the surface being destroyed is a blurred empty rectangle on
            // screen, and closeTimer below is set to match this.
            duration: Theme.reduceMotion ? 0 : (motion.closing ? 90 : 150)
            easing.type: motion.closing ? Easing.InCubic : Easing.OutCubic
        }
    }

    // Dismissal is deferred by exactly as long as the exit takes: the shell's
    // LazyLoader destroys the window the moment finished() lands, and a
    // destroyed window has no exit animation to play.
    function dismiss() {
        if (motion.closing)
            return;
        motion.closing = true;
        closeTimer.restart();
    }

    property Timer closeTimer: Timer {
        // The fade above, plus a frame. Longer would leave the surface mapped --
        // and therefore blurred -- after there was anything left to look at.
        interval: Theme.reduceMotion ? 1 : 105
        onTriggered: motion.finished()
    }

    // Component.onCompleted runs before the surface has been mapped, so setting
    // `revealed` there would let Qt settle both Behaviors at their resting
    // values before there is a frame to animate from. One turn of the event loop
    // puts the lifted, transparent frame on screen first; the entrance starts
    // from there.
    property Timer revealTimer: Timer {
        interval: 1
        running: true
        onTriggered: motion.revealed = true
    }
}
