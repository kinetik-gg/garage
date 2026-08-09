import QtQuick
import QtQuick.Controls as Controls

Controls.Slider {
    id: slider
    property string suffix: ""
    signal wheelAdjusted(real adjustedValue)
    implicitWidth: 190
    implicitHeight: 32
    from: 0
    to: 1

    WheelHandler {
        onWheel: event => {
            const direction = event.angleDelta.y > 0 ? 1 : -1;
            const increment = slider.stepSize > 0
                ? slider.stepSize : (slider.to - slider.from) / 20;
            slider.value = Math.max(slider.from,
                Math.min(slider.to, slider.value + direction * increment));
            slider.wheelAdjusted(slider.value);
            event.accepted = true;
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
