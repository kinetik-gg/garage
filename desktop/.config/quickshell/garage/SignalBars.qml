import QtQuick

// Four-step signal meter. Drawn rather than iconised because it has to track the
// theme continuously, and an svg per level would need four ColorOverlays to do
// what four rectangles do directly.
Row {
    id: bars

    property real strength: 0
    property bool muted: false

    // NetworkManager reports 0-100, but a binding that hands over a 0-1 fraction
    // would silently light no bars at all, so normalise here instead of trusting
    // every call site to know the scale.
    readonly property real level: strength > 1 ? strength : strength * 100

    readonly property var thresholds: [1, 35, 60, 80]

    width: 15
    height: 14
    spacing: 2

    Repeater {
        model: bars.thresholds.length

        Rectangle {
            required property int index
            anchors.bottom: parent.bottom
            width: 3
            height: 5 + index * 3
            radius: 1
            antialiasing: true
            color: bars.level >= bars.thresholds[index]
                ? (bars.muted ? Theme.textMuted : Theme.text) : Theme.hoverStrong
        }
    }
}
