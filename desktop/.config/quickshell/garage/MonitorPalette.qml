import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts

// The system panel: the machine's telemetry drawn at a readable size, plus the
// context the system icon only hints at -- containers and SMB shares.
//
// PaletteSurface hangs it inward from whichever edge owns the bar and centres
// it on the system widget's long-axis coordinate, clamped into the output. The
// next click outside dismisses it; its height still follows its contents.
//
// The graphs come from MetricsService, the one collector for the whole shell,
// and the stream's lifetime is this window's: acquire() as the panel maps,
// release() as it is destroyed, so nothing polls behind a popup nobody has
// open. The collector persists its own rolling history and replays it as the
// seed line on spawn, which is why a freshly opened panel draws full graphs
// rather than filling over two minutes. Containers and SMB read from
// BarContext, whose probes run 24/7 anyway to drive their extension state.
// Microphone privacy and AI quota state have dedicated extensions.
PaletteSurface {
    id: monitor
    surfaceNamespace: "garage-monitor"
    escapeEnabled: false

    // Set while the panel must not be taken down by an ambient dismissal -- the
    // shape the shell needs for the screenshot flow, where the panel being
    // photographed has to survive the pill and the capture. Honoured here for
    // Escape; the shell binds the shared DismissCatcher's `armed` to it for the
    // click outside, the way it already does for a capture in flight.
    property bool holdOpen: false

    readonly property int contentMargin: 12

    // The collector runs only while a reader holds it, and this window is
    // created on open and destroyed on close (the shell keeps its palettes in
    // LazyLoaders), so completed/destruction is exactly the open/close pair.
    Component.onCompleted: MetricsService.acquire()
    Component.onDestruction: MetricsService.release()

    // False until the collector has answered during this open. The seed
    // pre-populates the graphs, so what separates "the machine now" from "the
    // machine when the stream last ran" is this flag, and the header says which.
    property bool live: false

    Connections {
        target: MetricsService
        function onLatestChanged() {
            monitor.live = true;
        }
    }

    // -- Formatting ------------------------------------------------------------

    function formatBytes(bytes) {
        const value = MetricsService.number(bytes);
        if (isNaN(value) || value < 0)
            return "--";
        if (value >= 1073741824)
            return (value / 1073741824).toFixed(1) + " GiB";
        if (value >= 1048576)
            return (value / 1048576).toFixed(0) + " MiB";
        if (value >= 1024)
            return (value / 1024).toFixed(0) + " KiB";
        return value.toFixed(0) + " B";
    }

    function formatRate(bytesPerSecond) {
        const value = MetricsService.number(bytesPerSecond);
        if (isNaN(value) || value < 0)
            return "--";
        if (value >= 1073741824)
            return (value / 1073741824).toFixed(2) + " GiB/s";
        if (value >= 1048576)
            return (value / 1048576).toFixed(1) + " MiB/s";
        if (value >= 1024)
            return (value / 1024).toFixed(0) + " KiB/s";
        return value.toFixed(0) + " B/s";
    }

    // -- Readings --------------------------------------------------------------

    // Every reading in the stream is nullable: a box with no PSI has no memory
    // pressure, an Intel card reports no VRAM in use, a machine with no k10temp
    // has no package temperature. MetricsService.number is what stands between
    // the JSON and every label below -- nothing here may turn a missing reading
    // into a zero.
    readonly property var latest: MetricsService.latest

    readonly property var cpuInfo: monitor.latest && monitor.latest.cpu
        ? monitor.latest.cpu : null
    readonly property var memoryInfo: monitor.latest && monitor.latest.memory
        ? monitor.latest.memory : null
    readonly property var networkInfo: monitor.latest && monitor.latest.network
        ? monitor.latest.network : null
    readonly property var diskInfo: monitor.latest && monitor.latest.disk
        ? monitor.latest.disk : null
    readonly property var gpuInfo: MetricsService.primaryGpu(monitor.latest)
    readonly property int gpuCount: monitor.latest
        && Array.isArray(monitor.latest.gpus) ? monitor.latest.gpus.length : 0

    readonly property real cpuLoad: monitor.cpuInfo
        ? MetricsService.percent(monitor.cpuInfo.load) : NaN
    readonly property string cpuLoadAverage: {
        const load = monitor.cpuInfo ? monitor.cpuInfo.loadavg : null;
        if (!Array.isArray(load) || load.length < 3)
            return "";
        return "load " + Number(load[0]).toFixed(2)
            + " " + Number(load[1]).toFixed(2)
            + " " + Number(load[2]).toFixed(2);
    }

    readonly property real cpuCelsius: monitor.latest && monitor.latest.temp
        ? MetricsService.number(monitor.latest.temp.cpu_c) : NaN
    readonly property string cpuSensor: {
        const temp = monitor.latest ? monitor.latest.temp : null;
        const label = temp ? String(temp.label || "").trim() : "";
        return label !== "" ? label : "unknown sensor";
    }

    readonly property real memoryPercent: {
        const total = monitor.memoryInfo
            ? MetricsService.number(monitor.memoryInfo.total) : NaN;
        const used = monitor.memoryInfo
            ? MetricsService.number(monitor.memoryInfo.used) : NaN;
        if (isNaN(total) || total <= 0 || isNaN(used))
            return NaN;
        return used / total * 100;
    }
    readonly property real memoryPressure: monitor.memoryInfo
        ? MetricsService.number(monitor.memoryInfo.pressure_some_avg10) : NaN

    readonly property real networkDown: monitor.networkInfo
        ? MetricsService.number(monitor.networkInfo.rx_bps) : NaN
    readonly property real networkUp: monitor.networkInfo
        ? MetricsService.number(monitor.networkInfo.tx_bps) : NaN

    readonly property real diskRead: monitor.diskInfo
        ? MetricsService.number(monitor.diskInfo.read_bps) : NaN
    readonly property real diskWrite: monitor.diskInfo
        ? MetricsService.number(monitor.diskInfo.write_bps) : NaN

    readonly property real gpuLoad: monitor.gpuInfo
        ? MetricsService.number(monitor.gpuInfo.load) : NaN

    // Never "utilization" for an Intel card: the collector labels that reading
    // "activity (freq proxy)" because it is derived from the clock rather than
    // measured, and the panel repeats the label it was given rather than
    // inventing a more confident one.
    readonly property string gpuLoadKind: {
        const kind = monitor.gpuInfo
            ? String(monitor.gpuInfo.load_kind || "").trim() : "";
        return kind !== "" ? kind : "load unavailable";
    }

    readonly property string gpuVramLabel: {
        if (monitor.gpuInfo === null)
            return "";
        const total = MetricsService.number(monitor.gpuInfo.vram_total);
        const used = MetricsService.number(monitor.gpuInfo.vram_used);
        if (!isNaN(used) && !isNaN(total) && total > 0)
            return "vram " + monitor.formatBytes(used) + " / " + monitor.formatBytes(total);
        if (!isNaN(total) && total > 0)
            return "vram " + monitor.formatBytes(total);
        return "vram n/a";
    }

    // The one place the panel says where its figures came from. A seeded panel
    // is showing real history the collector persisted, which is worth having on
    // screen -- but it is not reporting at 1 Hz yet, and the difference between
    // "this is the machine now" and "this is stored history" is the whole
    // reason to read the header.
    readonly property string statusText: {
        if (MetricsService.error !== "")
            return MetricsService.error;
        if (!MetricsService.available)
            return "collector unavailable";
        if (monitor.live)
            return "1 Hz";
        return "connecting…";
    }

    // -- Surface ---------------------------------------------------------------

    // Two equal monitoring columns with enough plot width to remain readable,
    // clamped for small outputs rather than allowed to cross an edge.
    implicitWidth: {
        const target = monitor.effectiveScreen;
        const available = target ? target.width : 1920;
        return Math.max(1, Math.min(740, available - Theme.windowGutter * 2));
    }
    // Exactly its content, floored at 1px because a layer surface with no height
    // is not a surface the compositor can show and the column's implicit height
    // is zero for the frame before its children are laid out. Deliberately no
    // ceiling: a maximum would have to either clip a section away or scroll it
    // out of sight, and this panel is a set of readings that are all meant to be
    // readable at a glance.
    implicitHeight: Math.max(1, body.implicitHeight + monitor.contentMargin * 2)
    function requestDismissal() {
        if (!monitor.holdOpen)
            monitor.dismissSurface();
    }
    // Not yet in the compositor's glass layer_namespaces or its blur rules --
    // adding it there is part of wiring this panel up, and until it is there the
    // material is not drawn beneath this surface. Which is why the body below is
    // contentTint rather than bare glass: the panel is legible either way, and a
    // wall of small figures over live desktop content needs the body regardless.

    // One metric's block: the well every section is drawn in, so the sections
    // cannot drift apart. Same recessed hairline-framed shape SettingsGroup uses
    // for a group of controls.
    component Section: ContinuousRectangle {
        default property alias sectionData: sectionBody.data

        Layout.fillWidth: true
        implicitHeight: sectionBody.implicitHeight + 20
        radius: Theme.controlRadius
        power: Theme.cornerPower
        color: Theme.hover
        borderWidth: 1
        borderColor: Theme.frameInner

        ColumnLayout {
            id: sectionBody
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 10
            spacing: 6
        }
    }

    // A context section's header line: a label and one figure, the compact
    // shape the containers and SMB wells share so they read as one family.
    component ContextHeader: RowLayout {
        property string label: ""
        property string value: ""

        Layout.fillWidth: true
        spacing: 8

        Text {
            Layout.fillWidth: true
            text: parent.label
            color: Theme.text
            font.family: Theme.sans
            font.pixelSize: 13
            font.weight: Font.Medium
            elide: Text.ElideRight
            renderType: Text.NativeRendering
        }

        Text {
            text: parent.value
            color: Theme.textMuted
            font.family: Theme.mono
            font.pixelSize: 11
            renderType: Text.NativeRendering
        }
    }

    // A context section's body line, wrapped: names and share labels are
    // open-ended lists and a clipped one would hide exactly the entry that
    // made anybody open the panel.
    component ContextDetail: Text {
        Layout.fillWidth: true
        color: Theme.textDisabled
        font.family: Theme.sans
        font.pixelSize: 11
        wrapMode: Text.WordWrap
        renderType: Text.NativeRendering
    }

    ContinuousRectangle {
        id: panel
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        color: Theme.panel
        borderWidth: 1
        borderColor: Theme.frameOuter

        // The panel always fills its surface exactly. Anything less leaves bare
        // surface around it, and the compositor blurs that: an uncovered edge
        // does not read as empty, it reads as a second panel behind this one.
        // Which is why the entrance is a move and a fade and nothing else -- a
        // scale would shrink the panel inside its own surface for the length of
        // the animation and put that border back on screen every time it opened.
        opacity: monitor.contentOpacity


        // The body, over the glass and under everything else. Theme.panel is
        // transparent so the compositor's material shows through, and the
        // material alone is not a readable surface: over a bright window this
        // panel and its text wash out together. Declared before the content so
        // stacking order keeps it underneath without needing a z of its own.
        ContinuousRectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Theme.insetRadius(panel.radius, 1)
            power: Theme.cornerPower
            color: Theme.contentTint
        }

        // Inner hairline one inset px in from the outer one, the double frame
        // every other panel in the shell draws. insetRadius keeps the two
        // concentric at every corner radius setting.
        ContinuousRectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Theme.insetRadius(panel.radius, 1)
            power: Theme.cornerPower
            borderWidth: 1
            borderColor: Theme.frameInner
        }

        // The panel eats the clicks that land in the gaps between its sections
        // rather than leaving them unhandled.
        MouseArea {
            anchors.fill: parent
        }

        // Nothing scrolls and nothing is clipped. The window takes its height
        // from this column, so every section is on screen by construction and
        // there is no reading the panel can put out of reach.
        Item {
            id: content
            anchors.fill: parent
            anchors.margins: monitor.contentMargin

            ColumnLayout {
                id: body
                width: content.width
                spacing: 9

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12

                    ColumnLayout {
                        spacing: 2

                        Text {
                            text: "System"
                            color: Theme.text
                            font.family: Theme.sans
                            font.pixelSize: 17
                            font.weight: Font.DemiBold
                            renderType: Text.NativeRendering
                        }

                        Text {
                            text: "Rolling telemetry · two minutes per plot"
                            color: Theme.textDisabled
                            font.family: Theme.sans
                            font.pixelSize: 10
                            renderType: Text.NativeRendering
                        }
                    }

                    Item { Layout.fillWidth: true }

                    ContinuousRectangle {
                        Layout.preferredWidth: statusRow.implicitWidth + 20
                        Layout.preferredHeight: 28
                        radius: Theme.controlRadius
                        power: Theme.cornerPower
                        color: Theme.hover
                        borderWidth: 1
                        borderColor: Theme.frameInner

                        RowLayout {
                            id: statusRow
                            anchors.centerIn: parent
                            spacing: 7

                            Rectangle {
                                Layout.preferredWidth: 6
                                Layout.preferredHeight: 6
                                radius: 3
                                color: Theme.text
                                // Dim for anything that is not a live stream, so
                                // the pill reads as reporting or not reporting
                                // from across the room without the text.
                                opacity: monitor.live && MetricsService.error === ""
                                    ? 0.85 : 0.45
                            }

                            Text {
                                text: monitor.statusText
                                color: MetricsService.error !== ""
                                    ? Theme.text : Theme.textMuted
                                font.family: Theme.mono
                                font.pixelSize: 10
                                elide: Text.ElideRight
                                renderType: Text.NativeRendering
                            }
                        }
                    }
                }

                MenuSeparator {}

                GridLayout {
                    Layout.fillWidth: true
                    columns: 2
                    columnSpacing: 10
                    rowSpacing: 10
                    uniformCellWidths: true

                    // -- CPU ---------------------------------------------------
                    Section {
                        Layout.row: 0
                        Layout.column: 0
                        Layout.fillHeight: true

                        MetricGraphRow {
                            label: "CPU"
                            value: isNaN(monitor.cpuLoad)
                                ? "--" : monitor.cpuLoad.toFixed(0) + "%"
                            extra: monitor.cpuLoadAverage
                            points: MetricsService.cpuHistory
                            active: !isNaN(monitor.cpuLoad) && monitor.cpuLoad >= 25
                            graphHeight: 48
                        }

                        // Per-core load, one bar each. Fixed height whether or not
                        // there are any yet, so the section does not jump a row taller
                        // on the first snapshot.
                        Item {
                            Layout.fillWidth: true
                            implicitHeight: 16

                            RowLayout {
                                anchors.fill: parent
                                spacing: 2

                                Repeater {
                                    model: MetricsService.coreValues

                                    delegate: Item {
                                        required property var modelData
                                        // A core with no reading is drawn as idle
                                        // rather than as a gap: the row's job is to
                                        // show one core pinned against the rest, and a
                                        // missing slot would read as a missing core.
                                        readonly property real load: {
                                            const value = MetricsService.percent(modelData);
                                            return isNaN(value) ? 0 : value;
                                        }

                                        Layout.fillWidth: true
                                        // Equal preferred widths and fillWidth on all
                                        // of them is what divides the row evenly
                                        // without any child measuring the row it is
                                        // in.
                                        Layout.preferredWidth: 1
                                        Layout.fillHeight: true

                                        Rectangle {
                                            anchors.fill: parent
                                            radius: 1
                                            color: Theme.hoverStrong
                                        }

                                        Rectangle {
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            anchors.bottom: parent.bottom
                                            // A pixel of floor, so an idle core is a
                                            // slot rather than nothing at all.
                                            height: Math.max(1,
                                                parent.height * parent.load / 100)
                                            radius: 1
                                            // Monochrome like every graph above it,
                                            // and for the same reason: the accent
                                            // said nothing about a core's load that
                                            // its height was not already saying, and
                                            // it was the one coloured thing left in
                                            // the panel. The well behind it is
                                            // Theme.hoverStrong, which is this same
                                            // white at 9% -- so the pair is one
                                            // colour at two opacities.
                                            color: Theme.text
                                            opacity: 0.85
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // -- CPU temperature ---------------------------------------
                    Section {
                        Layout.row: 1
                        Layout.column: 1
                        Layout.fillHeight: true

                        MetricGraphRow {
                            label: "CPU Temperature"
                            value: isNaN(monitor.cpuCelsius)
                                ? "n/a" : monitor.cpuCelsius.toFixed(0) + "°C"
                            points: MetricsService.tempHistory
                            active: !isNaN(monitor.cpuCelsius) && monitor.cpuCelsius >= 75
                            detail: monitor.cpuSensor + " · 0–100°C"
                            // A machine with no package sensor has nothing to collect,
                            // so it says that instead of promising a line later.
                            idleLabel: monitor.latest !== null && isNaN(monitor.cpuCelsius)
                                ? "no sensor" : "collecting…"
                        }
                    }

                    // -- Memory ------------------------------------------------
                    Section {
                        Layout.row: 1
                        Layout.column: 0
                        Layout.fillHeight: true

                        MetricGraphRow {
                            label: "Memory"
                            value: isNaN(monitor.memoryPercent)
                                ? "--" : monitor.memoryPercent.toFixed(0) + "%"
                            // PSI some avg10: the share of the last ten seconds in
                            // which at least one task was stalled waiting on memory.
                            // It moves before anything is visibly wrong, which is why
                            // it is here next to a usage figure that does not.
                            extra: isNaN(monitor.memoryPressure)
                                ? "pressure n/a"
                                : "pressure " + monitor.memoryPressure.toFixed(2) + "%"
                            points: MetricsService.memoryHistory
                            active: !isNaN(monitor.memoryPercent) && monitor.memoryPercent >= 70
                            detail: monitor.memoryInfo === null ? ""
                                : monitor.formatBytes(monitor.memoryInfo.used) + " of "
                                    + monitor.formatBytes(monitor.memoryInfo.total)
                                    + " · " + monitor.formatBytes(monitor.memoryInfo.available)
                                    + " available"
                        }
                    }

                    // -- Network -----------------------------------------------
                    Section {
                        Layout.row: 2
                        Layout.column: 0
                        Layout.fillHeight: true

                        MetricGraphRow {
                            label: "Network"
                            value: "↓ " + monitor.formatRate(monitor.networkDown)
                            extra: "↑ " + monitor.formatRate(monitor.networkUp)
                            points: MetricsService.networkDownHistory
                            // Up as the second line, no fill and a dimmer stroke: it
                            // shares the axis with down and is the subordinate of the
                            // two on a desktop.
                            secondaryPoints: MetricsService.networkUpHistory
                            active: !isNaN(monitor.networkDown) && !isNaN(monitor.networkUp)
                                && (monitor.networkDown + monitor.networkUp)
                                    / MetricsService.mib >= 0.1
                            detail: {
                                const iface = monitor.networkInfo
                                    ? String(monitor.networkInfo.iface || "").trim() : "";
                                return (iface !== "" ? iface : "no default route")
                                    + " · down over up · log scale to 2 GiB/s";
                            }
                        }
                    }

                    // -- Disk --------------------------------------------------
                    Section {
                        Layout.row: 2
                        Layout.column: 1
                        Layout.fillHeight: true

                        MetricGraphRow {
                            label: "Disk"
                            // Same two glyphs the network row uses, rather than an
                            // r/w of its own: a pair of throughput figures reads the
                            // same way everywhere in the shell, and the detail line
                            // below says which is which.
                            value: "↓ " + monitor.formatRate(monitor.diskRead)
                            extra: "↑ " + monitor.formatRate(monitor.diskWrite)
                            // Read and write are graphed as one line rather than two.
                            // That is what the collector stores and therefore what the
                            // seed can restore: splitting the line would mean opening
                            // this section empty every time, and the two figures above
                            // are where the split actually gets read.
                            points: MetricsService.diskHistory
                            active: !isNaN(monitor.diskRead) && !isNaN(monitor.diskWrite)
                                && (monitor.diskRead + monitor.diskWrite)
                                    / MetricsService.mib >= 1
                            detail: {
                                const device = monitor.diskInfo
                                    ? String(monitor.diskInfo.device || "").trim() : "";
                                const read = isNaN(monitor.diskRead) ? 0 : monitor.diskRead;
                                const write = isNaN(monitor.diskWrite) ? 0 : monitor.diskWrite;
                                return (device !== "" ? device : "no block device")
                                    + " · read over write · total "
                                    + monitor.formatRate(read + write)
                                    + " · log scale to 2 GiB/s";
                            }
                        }
                    }

                    // -- GPU ---------------------------------------------------
                    Section {
                        Layout.row: 0
                        Layout.column: 1
                        Layout.fillHeight: true

                        MetricGraphRow {
                            label: "GPU"
                            value: isNaN(monitor.gpuLoad)
                                ? "n/a" : monitor.gpuLoad.toFixed(0) + "%"
                            extra: monitor.gpuVramLabel
                            points: MetricsService.gpuHistory
                            secondaryPoints: MetricsService.gpuVramHistory
                            active: !isNaN(monitor.gpuLoad) && monitor.gpuLoad >= 10
                            detail: {
                                if (monitor.gpuInfo === null)
                                    return monitor.latest === null ? "" : "no GPU found";
                                const parts = [
                                    String(monitor.gpuInfo.name || "GPU").trim(),
                                    monitor.gpuLoadKind
                                ];
                                const celsius = MetricsService.number(monitor.gpuInfo.temp_c);
                                if (!isNaN(celsius))
                                    parts.push(celsius.toFixed(0) + "°C");
                                if (monitor.gpuCount > 1)
                                    parts.push("+" + (monitor.gpuCount - 1) + " more GPU");
                                return parts.join(" · ");
                            }
                            idleLabel: monitor.latest !== null && monitor.gpuInfo === null
                                ? "no GPU" : "collecting…"
                        }
                    }

                    // -- Containers --------------------------------------------
                    // The context the bar's chips used to carry, from the same
                    // BarContext probes that still drive the icon's badge. In
                    // the grid rather than a second list so the panel stays one
                    // even surface of wells.
                    Section {
                        Layout.row: 3
                        Layout.column: 0
                        Layout.fillHeight: true

                        ContextHeader {
                            label: "Containers"
                            value: BarContext.containersAvailable
                                ? BarContext.containerCount + " running"
                                : "no engine"
                        }

                        ContextDetail {
                            text: {
                                if (!BarContext.containersAvailable)
                                    return "No container engine answered the probe.";
                                if (BarContext.containerNames.length === 0)
                                    return "Nothing running.";
                                return BarContext.containerNames.join(" · ");
                            }
                        }
                    }

                    // -- SMB shares --------------------------------------------
                    Section {
                        Layout.row: 3
                        Layout.column: 1
                        Layout.fillHeight: true

                        ContextHeader {
                            label: "SMB Shares"
                            value: BarContext.smbAvailable
                                ? BarContext.smbConnected + " of "
                                    + BarContext.smbExpected + " connected"
                                : "not probed"
                        }

                        ContextDetail {
                            text: {
                                if (!BarContext.smbAvailable)
                                    return "No shares are expected on this machine.";
                                if (BarContext.smbMissingLabels.length === 0)
                                    return "Every expected share is mounted.";
                                return "Missing: "
                                    + BarContext.smbMissingLabels.join(" · ");
                            }
                        }
                    }

                }
            }
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: monitor.requestDismissal()
    }
}
