import QtQuick
import QtQuick.Controls as Controls

// A slider whose wheel is not optional.
//
// Scrolling used to report through a wheelAdjusted signal of this file's own,
// which every consumer had to connect by hand *in addition to* the drag handler
// it already had. Two handlers for one control means a slider that forgets the
// second one silently ignores the wheel, and nothing anywhere says so. A notch
// now arrives as the same signals a drag does -- moved() while it happens,
// committed() when the gesture is over -- so a consumer that handles dragging
// has already handled scrolling, and a slider written tomorrow cannot forget.
Controls.Slider {
    id: slider
    property string suffix: ""

    // The end of a gesture rather than the middle of one: a handle let go, a
    // wheel notch, an arrow key. What consumers that write to disk or shell out
    // per value want -- moved() fires on every frame of a drag, which for a
    // preference is a subprocess per pixel.
    signal committed(real value)

    // What one notch moves: the slider's own step where it has one, a twentieth
    // of the range otherwise, so the wheel crosses any track in twenty notches
    // whatever its units are.
    readonly property real wheelStep: slider.stepSize > 0
        ? slider.stepSize : (slider.to - slider.from) / 20

    implicitWidth: 190
    implicitHeight: 32
    from: 0
    to: 1

    // The far end of a drag. Dragging emits moved() the whole way down, so the
    // release is the only point in it that is a commit.
    onPressedChanged: if (!slider.pressed) slider.committed(slider.value)

    // A notch is a step, exactly as an arrow key is: it moves the handle and
    // reports that it moved. The commit follows from the Connections below,
    // which is the same route the keys take, so there is one definition of what
    // a discrete step does rather than one per input device.
    WheelHandler {
        enabled: slider.enabled
        onWheel: event => {
            event.accepted = true;
            const direction = event.angleDelta.y > 0 ? 1 : -1;
            const next = Math.max(slider.from, Math.min(slider.to,
                slider.value + direction * slider.wheelStep));
            if (next === slider.value)
                return;
            slider.value = next;
            slider.moved();
        }
    }

    // Connections rather than an onMoved handler on the root: a consumer that
    // wants the live value writes its own onMoved (the volume slider does), and
    // this must keep working alongside it rather than be shadowed by it.
    //
    // Not while the handle is held: that is a drag in progress, and its commit
    // is the release above.
    Connections {
        target: slider
        function onMoved() {
            if (!slider.pressed)
                slider.committed(slider.value);
        }
    }

    background: ContinuousRectangle {
        id: track
        x: slider.leftPadding
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        width: slider.availableWidth
        height: 6
        radius: height
        color: Theme.hoverStrong

        ContinuousRectangle {
            width: slider.visualPosition * parent.width
            height: parent.height
            radius: track.radius
            color: Theme.accent
        }
    }

    handle: Rectangle {
        x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        implicitWidth: 18
        implicitHeight: 18
        radius: width / 2
        color: slider.pressed ? Theme.text : Theme.knob
        border.width: 1
        border.color: Theme.frameOuter
        antialiasing: true
    }
}
