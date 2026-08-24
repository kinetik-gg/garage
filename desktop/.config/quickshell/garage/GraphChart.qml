import QtQuick
import QtQuick.Shapes

// One time-series strip, shared by the shell's metrics service and System panel.
//
// The geometry is a port of garage-metrics' render_widget -- the same constants,
// because the same collector feeds both surfaces and a graph that disagreed with
// the bar about how a number looks would be the shell contradicting itself. The
// series is 0..100 and nothing else: the collector normalises every metric into
// that range before it stores or streams it (percentages directly, throughput on
// a log curve), so this file never has to know which metric it is holding.
//
// Shape + ShapePath + PathSvg with the path built as a string in JS, the way
// ContinuousRectangle draws its superellipse, rather than Canvas. A Canvas is a
// separate render target and a repaint through a JS paint handler per frame, and
// there are seven of these on screen being fed at 1 Hz.
Item {
    id: chart

    // The series, oldest sample first. Reassign it; a mutated array does not
    // notify, so an in-place push would leave the path bound to a stale path
    // string until something else in it changed.
    property var points: []

    // An optional second series, drawn with no fill and a thinner, dimmer stroke.
    // Two 8% fills stack into a muddy block where they overlap, and the second
    // line is meant to read as subordinate to the first -- network up under down,
    // VRAM under GPU load. Same reasoning, same numbers, as the bar's history2.
    property var secondaryPoints: []

    // The one colour every part of a graph is drawn in. Monochrome on purpose:
    // this used to default to Theme.accent, and an activity panel tinted with
    // the user's accent read as though the colour meant something -- it does
    // not, every strip here is the same kind of number. So a graph is the
    // shell's own foreground token (white on the dark scheme, near-black on the
    // light one) and the five opacities below are the only thing that separates
    // fill from stroke from axis. Overridable per instance, but no call site in
    // the shell needs to: the default is the treatment.
    property color ink: Theme.text

    // The bar dims a strip whose metric is idle rather than hiding it, so a row
    // of graphs reads at a glance as "this one is doing something". Same two
    // opacities.
    property bool active: false
    readonly property real strokeOpacity: chart.active ? 0.92 : 0.62

    // What to say before there is anything to draw. A metric with no seed in the
    // bar's history files -- or no sensor at all -- has to say so; an empty frame
    // with a baseline in it looks like a reading of zero.
    property string idleLabel: "collecting…"

    property real strokeWidth: 1.4
    property real secondaryStrokeWidth: 1.1
    property real fillOpacity: 0.08
    // The second series' stroke. Named alongside the rest rather than left as a
    // literal in the ShapePath below, because with one colour for the whole
    // chart these five numbers are the entire visual language of it.
    property real secondaryOpacity: 0.34
    property real midlineOpacity: 0.10
    property real baselineOpacity: 0.16

    // Opt-in because the activity charts intentionally retain the straight-line
    // geometry of garage-metrics. The media spectrum enables this to turn its
    // closely spaced frequency bands into one continuous cubic Bézier curve.
    property bool smoothCurve: false

    implicitWidth: 160
    implicitHeight: 40

    // Half a stroke of margin on every side the line can reach: a sample at 100
    // sits on the top edge and a round cap centred there would lose its top half
    // to the item's bounds.
    readonly property real plotLeft: chart.strokeWidth / 2
    readonly property real plotRight: Math.max(chart.plotLeft, chart.width - chart.strokeWidth / 2)
    readonly property real plotTop: chart.strokeWidth / 2
    readonly property real plotBottom: Math.max(chart.plotTop, chart.height - chart.strokeWidth / 2)
    readonly property real plotHeight: chart.plotBottom - chart.plotTop

    readonly property int pointCount: Array.isArray(chart.points) ? chart.points.length : 0
    readonly property bool hasData: chart.pointCount > 0
    readonly property bool hasSecondary: Array.isArray(chart.secondaryPoints)
        && chart.secondaryPoints.length > 0

    // A degenerate but syntactically valid path, so a ShapePath is never handed
    // an empty string while the shape it belongs to is being hidden.
    readonly property string emptyPath: "M 0 0"

    // Sanitised copy of a series: numbers, clamped, non-finite treated as zero.
    // The stream carries nulls wherever a sensor has nothing to say and the
    // seeded history is whatever the bar last wrote, so neither can be trusted
    // to be plottable as it arrives.
    // Plotted straight. Every series that needs a logarithm arrives with one
    // already applied -- throughput goes through logScale() as it is sampled --
    // and the rest are percentages, which are already the scale they are read
    // on. Curving a percentage a second time put 1% of load two thirds of the
    // way up the plot.
    function plottedValue(sourceValue) {
        const value = Number(sourceValue);
        return isFinite(value) ? Math.max(0, Math.min(100, value)) : 0;
    }

    function series(source) {
        if (!Array.isArray(source) || source.length === 0)
            return [];
        const values = [];
        for (let index = 0; index < source.length; ++index)
            values.push(chart.plottedValue(source[index]));
        // One sample is not a line. Doubling it draws the flat line that value
        // deserves rather than nothing at all, which is what the bar does with a
        // one-entry history for the same reason.
        if (values.length === 1)
            values.push(values[0]);
        return values;
    }

    // Oldest sample at the left edge, newest at the right, stretched across the
    // full width however many there are -- the bar's `index * width / (len - 1)`.
    // A partial series therefore fills the frame and rescales as it grows instead
    // of hanging off one edge, and once the window is full (120 samples) it
    // scrolls the way the strip does.
    function linePath(source) {
        const values = chart.series(source);
        if (values.length === 0)
            return chart.emptyPath;
        const span = values.length - 1;
        const width = chart.plotRight - chart.plotLeft;
        const coordinates = [];
        for (let index = 0; index < values.length; ++index) {
            coordinates.push({
                x: chart.plotLeft + index * width / span,
                y: chart.plotBottom - values[index] / 100 * chart.plotHeight
            });
        }

        let path = "M " + coordinates[0].x.toFixed(2) + " "
            + coordinates[0].y.toFixed(2);
        if (!chart.smoothCurve) {
            for (let index = 1; index < coordinates.length; ++index)
                path += " L " + coordinates[index].x.toFixed(2) + " "
                    + coordinates[index].y.toFixed(2);
            return path;
        }

        // Convert a Catmull-Rom spline to cubic Bézier segments. Repeating the
        // first/last points supplies stable endpoint tangents. Control-point Y
        // values are clamped to the plot so even a large adjacent-band jump can
        // never make the curve escape the chart vertically.
        for (let index = 0; index < coordinates.length - 1; ++index) {
            const previous = index > 0 ? coordinates[index - 1] : coordinates[index];
            const current = coordinates[index];
            const next = coordinates[index + 1];
            const following = index + 2 < coordinates.length
                ? coordinates[index + 2] : next;
            const control1X = current.x + (next.x - previous.x) / 6;
            const control1Y = Math.max(chart.plotTop, Math.min(chart.plotBottom,
                current.y + (next.y - previous.y) / 6));
            const control2X = next.x - (following.x - current.x) / 6;
            const control2Y = Math.max(chart.plotTop, Math.min(chart.plotBottom,
                next.y - (following.y - current.y) / 6));
            path += " C " + Number(control1X).toFixed(2) + " "
                + Number(control1Y).toFixed(2) + " "
                + Number(control2X).toFixed(2) + " "
                + Number(control2Y).toFixed(2) + " "
                + Number(next.x).toFixed(2) + " " + Number(next.y).toFixed(2);
        }
        return path;
    }

    // The line, dropped to the baseline at both ends and closed. Reuses linePath
    // rather than walking the series a second time, so the fill cannot end up a
    // pixel out from the stroke it sits under.
    function areaPath(source) {
        const line = chart.linePath(source);
        if (line === chart.emptyPath)
            return chart.emptyPath;
        return line
            + " L " + chart.plotRight.toFixed(2) + " " + chart.plotBottom.toFixed(2)
            + " L " + chart.plotLeft.toFixed(2) + " " + chart.plotBottom.toFixed(2)
            + " Z";
    }

    // Midline and baseline, as rectangles rather than as paths in the shape
    // below: they are two horizontal hairlines, and a 1px rectangle lands on the
    // pixel grid where an antialiased stroke does not. Drawn whether or not there
    // is data, so a graph that is still collecting keeps the height and the frame
    // it will have once it fills in.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        y: Math.round(chart.plotTop + chart.plotHeight / 2)
        height: 1
        color: chart.ink
        opacity: chart.midlineOpacity
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        y: Math.round(chart.plotBottom)
        height: 1
        color: chart.ink
        opacity: chart.baselineOpacity
    }

    Shape {
        anchors.fill: parent
        visible: chart.hasData
        antialiasing: true
        preferredRendererType: Shape.CurveRenderer

        // Fill first, so the stroke draws over its top edge rather than under it.
        ShapePath {
            fillColor: Qt.rgba(chart.ink.r, chart.ink.g, chart.ink.b,
                chart.fillOpacity)
            strokeColor: "transparent"
            strokeWidth: 0
            PathSvg { path: chart.areaPath(chart.points) }
        }

        ShapePath {
            fillColor: "transparent"
            strokeColor: Qt.rgba(chart.ink.r, chart.ink.g, chart.ink.b,
                chart.strokeOpacity)
            strokeWidth: chart.strokeWidth
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: chart.linePath(chart.points) }
        }

        ShapePath {
            fillColor: "transparent"
            strokeColor: Qt.rgba(chart.ink.r, chart.ink.g, chart.ink.b,
                chart.secondaryOpacity)
            strokeWidth: chart.secondaryStrokeWidth
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg {
                path: chart.hasSecondary
                    ? chart.linePath(chart.secondaryPoints) : chart.emptyPath
            }
        }
    }

    Text {
        anchors.centerIn: parent
        visible: !chart.hasData
        text: chart.idleLabel
        color: Theme.textDisabled
        font.family: Theme.sans
        font.pixelSize: 10
        renderType: Text.NativeRendering
    }
}
