import QtQuick
import QtQuick.Layouts

// One metric in the monitor: its name, its current reading, and its graph.
//
// The layout the bar's strips use, unfolded. On the bar a strip is one line --
// label, graph, value, extra -- because it has 22px of height and 44px of graph
// to work with. Here the label row sits above a graph wide enough to read, and
// the third line carries whatever the bar could only put in a tooltip.
//
// Numbers are set in Geist Mono on purpose: these are figures that change every
// second, and a proportional font makes the row twitch sideways as digits change
// width. Labels stay in Plus Jakarta Sans like every other label in the shell.
ColumnLayout {
    id: row

    property string label: ""

    // The headline reading, right-aligned at the end of the label row.
    property string value: ""

    // A second reading that belongs to the same metric -- upload against
    // download, VRAM against GPU load. Hidden when empty rather than reserving
    // the space, because half the metrics have no honest second number.
    property string extra: ""

    // The line under the graph: what the bar had to hide in a tooltip. Device
    // names, sensor names, totals, and the note that a scale is logarithmic.
    property string detail: ""

    property var points: []
    property var secondaryPoints: []
    // Passed straight through to GraphChart, whose default is already the
    // monochrome treatment the whole panel uses -- see the note on `ink` there.
    // Named the same as the property it feeds so a reader does not have to work
    // out which colour of the two this one is.
    property color ink: Theme.text
    property bool active: false
    property string idleLabel: "collecting…"
    property real graphHeight: 40

    Layout.fillWidth: true
    spacing: 5

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        Text {
            text: row.label
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 11
            font.weight: Font.DemiBold
            renderType: Text.NativeRendering
        }

        Item { Layout.fillWidth: true }

        Text {
            text: row.value
            color: Theme.text
            font.family: Theme.mono
            font.pixelSize: 12
            renderType: Text.NativeRendering
        }

        // Capped and elided rather than left to size itself: the second reading
        // is the longest string in the row on the metrics that have one, and a
        // row that overflows would push the headline value off the panel.
        Text {
            Layout.maximumWidth: 200
            visible: row.extra !== ""
            text: row.extra
            color: Theme.textMuted
            font.family: Theme.mono
            font.pixelSize: 11
            elide: Text.ElideRight
            renderType: Text.NativeRendering
        }
    }

    GraphChart {
        Layout.fillWidth: true
        Layout.preferredHeight: row.graphHeight
        points: row.points
        secondaryPoints: row.secondaryPoints
        ink: row.ink
        active: row.active
        idleLabel: row.idleLabel
    }

    Text {
        Layout.fillWidth: true
        visible: row.detail !== ""
        text: row.detail
        color: Theme.textDisabled
        font.family: Theme.sans
        font.pixelSize: 10
        elide: Text.ElideRight
        renderType: Text.NativeRendering
    }
}
