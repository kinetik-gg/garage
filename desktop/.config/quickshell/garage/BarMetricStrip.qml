import QtQuick
import QtQuick.Shapes

// One metric strip, drawn to the old SVG strip's own spec: a 14px Phosphor
// icon at 75% strength, a 22x16 sparkline, and the value beside them in
// 11.5px Plus Jakarta Sans 600 at full foreground. The whole 22px band is
// what the old image module shipped, sized 1:1 in a taller bar; it centres
// vertically here like every other widget.
//
// The network strip keeps its old shape too: two direction arrows with a
// rate each, no graph -- the old layout table drew it that way.
//
// Repaint wiring note, because this is the second time it matters: the
// sparkline lives on the INNER canvas, so every data-driven repaint must
// call graph.requestPaint() -- the root's requestPaint is a no-op for it.
Item {
    id: strip

    property string name: ""
    property var series: []
    property string value: "--"
    property string tip: ""
    signal activated()

    readonly property bool isNetwork: name === "network"
    readonly property int graphWidth: 22
    readonly property int graphHeight: 16
    readonly property int iconSize: 14

    implicitWidth: content.implicitWidth + BarState.scaled("image") * 2
    implicitHeight: 22

    // Old hover: an 8px-radius tint at 12% over the whole module.
    Rectangle {
        anchors.fill: parent
        radius: 8
        color: clickArea.pressed ? Qt.alpha(Theme.text, 0.22)
            : clickArea.containsMouse ? Qt.alpha(Theme.text, 0.12) : "transparent"

        Behavior on color {
            ColorAnimation { duration: Theme.reduceMotion ? 0 : 130 }
        }
    }

    Row {
        id: content

        anchors.centerIn: parent
        spacing: 6

        // The metric's Phosphor glyph, as the old strip drew it: the icon's
        // own 256-unit path scaled into a 14px box at 75% strength.
        Shape {
            visible: !strip.isNetwork
            anchors.verticalCenter: parent.verticalCenter
            width: strip.iconSize
            height: strip.iconSize
            antialiasing: true
            smooth: true

            transform: Scale {
                xScale: strip.iconSize / 256
                yScale: strip.iconSize / 256
            }

            ShapePath {
                fillColor: Qt.alpha(Theme.text, 0.75)
                strokeColor: "transparent"
                strokeWidth: -1
                PathSvg { path: strip.iconPath(strip.name) }
            }
        }

        // The down/up arrow pair and the two rates, for the network strip.
        Row {
            visible: strip.isNetwork
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4

            Shape {
                anchors.verticalCenter: parent.verticalCenter
                width: strip.iconSize
                height: strip.iconSize
                antialiasing: true

                transform: Scale {
                    xScale: strip.iconSize / 256
                    yScale: strip.iconSize / 256
                }

                ShapePath {
                    fillColor: Qt.alpha(Theme.text, 0.75)
                    strokeColor: "transparent"
                    strokeWidth: -1
                    PathSvg { path: strip.iconPath("arrow-down") }
                }
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: MetricsService.rateLabel(MetricsService.downRate())
                color: Theme.text
                font.family: Theme.sans
                font.pixelSize: 12
                font.weight: Font.DemiBold
                renderType: Text.NativeRendering
            }

            Shape {
                anchors.verticalCenter: parent.verticalCenter
                width: strip.iconSize
                height: strip.iconSize
                antialiasing: true

                transform: Scale {
                    xScale: strip.iconSize / 256
                    yScale: strip.iconSize / 256
                }

                ShapePath {
                    fillColor: Qt.alpha(Theme.text, 0.75)
                    strokeColor: "transparent"
                    strokeWidth: -1
                    PathSvg { path: strip.iconPath("arrow-up") }
                }
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: MetricsService.rateLabel(MetricsService.upRate())
                color: Theme.text
                font.family: Theme.sans
                font.pixelSize: 12
                font.weight: Font.DemiBold
                renderType: Text.NativeRendering
            }
        }

        // The sparkline, on the canvas that actually paints it.
        Canvas {
            id: graph

            visible: !strip.isNetwork
            anchors.verticalCenter: parent.verticalCenter
            width: strip.graphWidth
            height: strip.graphHeight
            antialiasing: true

            onPaint: {
                const ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);
                ctx.lineWidth = 1.5;
                ctx.lineJoin = "round";
                ctx.strokeStyle = Qt.rgba(Theme.text.r, Theme.text.g, Theme.text.b, 0.9);
                const points = strip.series;
                if (!Array.isArray(points) || points.length === 0)
                    return;
                ctx.beginPath();
                if (points.length === 1) {
                    // One sample: a flat baseline at its level, not nothing.
                    const y = height - Math.max(0, Math.min(100, points[0])) / 100
                        * (height - 2) - 1;
                    ctx.moveTo(0, y);
                    ctx.lineTo(width, y);
                    ctx.stroke();
                    return;
                }
                const step = width / (points.length - 1);
                for (let index = 0; index < points.length; ++index) {
                    const px = index * step;
                    const py = height
                        - Math.max(0, Math.min(100, points[index])) / 100
                        * (height - 2) - 1;
                    if (index === 0)
                        ctx.moveTo(px, py);
                    else
                        ctx.lineTo(px, py);
                }
                ctx.stroke();
            }

            Component.onCompleted: graph.requestPaint()
            onWidthChanged: graph.requestPaint()

            Connections {
                target: strip
                function onSeriesChanged() { graph.requestPaint(); }
                function onVisibleChanged() { if (strip.visible) graph.requestPaint(); }
            }

            Connections {
                target: Theme
                function onSchemeChanged() { graph.requestPaint(); }
            }
        }

        Text {
            visible: !strip.isNetwork
            anchors.verticalCenter: parent.verticalCenter
            text: strip.value
            color: Theme.text
            font.family: Theme.sans
            font.pixelSize: 12
            font.weight: Font.DemiBold
            renderType: Text.NativeRendering
        }
    }

    // The old strips' Phosphor paths, verbatim from the collector's icon
    // table (garage-metrics data.rs): 256-unit view space, filled.
    function iconPath(key) {
        const paths = {
            cpu: "M152,96H104a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8h48a8,8,0,0,0,8-8V104A8,8,0,0,0,152,96Zm-8,48H112V112h32Zm88,0H216V112h16a8,8,0,0,0,0-16H216V56a16,16,0,0,0-16-16H160V24a8,8,0,0,0-16,0V40H112V24a8,8,0,0,0-16,0V40H56A16,16,0,0,0,40,56V96H24a8,8,0,0,0,0,16H40v32H24a8,8,0,0,0,0,16H40v40a16,16,0,0,0,16,16H96v16a8,8,0,0,0,16,0V216h32v16a8,8,0,0,0,16,0V216h40a16,16,0,0,0,16-16V160h16a8,8,0,0,0,0-16Zm-32,56H56V56H200v95.87s0,.09,0,.13,0,.09,0,.13V200Z",
            memory: "M232,56H24A16,16,0,0,0,8,72V200a8,8,0,0,0,16,0V184H40v16a8,8,0,0,0,16,0V184H72v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V184h16v16a8,8,0,0,0,16,0V72A16,16,0,0,0,232,56ZM24,72H232v96H24Zm88,80a8,8,0,0,0,8-8V96a8,8,0,0,0-8-8H48a8,8,0,0,0-8,8v48a8,8,0,0,0,8,8ZM56,104h48v32H56Zm88,48h64a8,8,0,0,0,8-8V96a8,8,0,0,0-8-8H144a8,8,0,0,0-8,8v48A8,8,0,0,0,144,152Zm8-48h48v32H152Z",
            temp: "M136,153V88a8,8,0,0,0-16,0v65a32,32,0,1,0,16,0Zm-8,47a16,16,0,1,1,16-16A16,16,0,0,1,128,200Zm40-66V48a40,40,0,0,0-80,0v86a64,64,0,1,0,80,0Zm-40,98a48,48,0,0,1-27.42-87.4A8,8,0,0,0,104,138V48a24,24,0,0,1,48,0v90a8,8,0,0,0,3.42,6.56A48,48,0,0,1,128,232Z",
            disk: "M208,136H48a16,16,0,0,0-16,16v48a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V152A16,16,0,0,0,208,136Zm0,64H48V152H208v48Zm0-160H48A16,16,0,0,0,32,56v48a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V56A16,16,0,0,0,208,40Zm0,64H48V56H208v48ZM192,80a12,12,0,1,1-12-12A12,12,0,0,1,192,80Zm0,96a12,12,0,1,1-12-12A12,12,0,0,1,192,176Z",
            gpu: "M232,48H16a8,8,0,0,0-8,8V208a8,8,0,0,0,16,0V192H40v16a8,8,0,0,0,16,0V192H72v16a8,8,0,0,0,16,0V192h16v16a8,8,0,0,0,16,0V192H232a16,16,0,0,0,16-16V64A16,16,0,0,0,232,48Zm0,128H24V64H232Zm-56-16a40,40,0,1,0-40-40A40,40,0,0,0,176,160Zm-24-40a23.74,23.74,0,0,1,2.35-10.34l32,32A23.74,23.74,0,0,1,176,144,24,24,0,0,1,152,120Zm48,0a23.74,23.74,0,0,1-2.35,10.34l-32-32A23.74,23.74,0,0,1,176,96,24,24,0,0,1,200,120ZM80,160a40,40,0,1,0-40-40A40,40,0,0,0,80,160ZM56,120a23.74,23.74,0,0,1,2.35-10.34l32,32A23.74,23.74,0,0,1,80,144,24,24,0,0,1,56,120Zm48,0a23.74,23.74,0,0,1-2.35,10.34l-32-32A23.74,23.74,0,0,1,80,96,24,24,0,0,1,104,120Z",
            "arrow-down": "M205.66,122.34l-72,72a8,8,0,0,1-11.32,0l-72-72a8,8,0,0,1,11.32-11.32L120,169.37V32a8,8,0,0,1,16,0V169.37L194.34,111a8,8,0,0,1,11.32,11.32Z",
            "arrow-up": "M205.66,133.66a8,8,0,0,1-11.32,0L136,75.31V224a8,8,0,0,1-16,0V75.31L61.66,133.66a8,8,0,0,1-11.32-11.32l72-72a8,8,0,0,1,11.32,0l72,72A8,8,0,0,1,205.66,133.66Z"
        };
        return paths[key] || "";
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
