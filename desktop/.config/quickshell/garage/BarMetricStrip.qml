import QtQuick

// One metric strip: a sparkline of the last N samples, the current value beside
// it, and a tooltip naming the figure. The data arrives from MetricsService's
// single stream -- this draws whatever series it is handed, one point per pixel,
// exactly the information design the old SVG strips carried.
Canvas {
    id: strip

    property string name: ""
    property var series: []
    property string value: "--"
    property string tip: ""
    signal activated()

    readonly property int graphWidth: 44
    readonly property int graphHeight: 18
    implicitWidth: graphWidth + valueText.implicitWidth + 6 + BarState.scaled("image") * 2
    implicitHeight: BarState.height - 12

    onSeriesChanged: requestPaint()
    onWidthChanged: requestPaint()

    Canvas {
        id: graph

        x: BarState.scaled("image")
        anchors.verticalCenter: parent.verticalCenter
        width: strip.graphWidth
        height: strip.graphHeight

        onPaint: {
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            const points = strip.series;
            if (!Array.isArray(points) || points.length < 2)
                return;
            ctx.beginPath();
            const step = width / (points.length - 1);
            for (let index = 0; index < points.length; ++index) {
                const px = index * step;
                const py = height - Math.max(0, Math.min(100, points[index])) / 100 * (height - 2) - 1;
                if (index === 0)
                    ctx.moveTo(px, py);
                else
                    ctx.lineTo(px, py);
            }
            ctx.strokeStyle = Qt.rgba(
                Theme.text.r, Theme.text.g, Theme.text.b, 0.9);
            ctx.lineWidth = 1;
            ctx.stroke();
        }

        Connections {
            target: Theme
            function onSchemeChanged() { graph.requestPaint(); }
        }
    }

    Text {
        id: valueText

        anchors.verticalCenter: parent.verticalCenter
        x: strip.graphWidth + BarState.scaled("image") * 2 + 6
        text: strip.value
        color: Theme.textMuted
        font.family: Theme.mono
        font.pixelSize: 11
        renderType: Text.NativeRendering
    }

    MouseArea {
        id: clickArea

        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton
        onClicked: strip.activated()
    }

    BarTip {
        owner: strip
        text: strip.tip
        opacity: strip.tip !== "" && clickArea.containsMouse ? 1 : 0
    }
}
