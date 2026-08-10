import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts

// The activity monitor: the six numbers the bar keeps in 44px strips, drawn at a
// size they can be read at.
//
// A right-anchored layer surface, the same idiom as the control centre next to
// it: a panel the next click outside dismisses, above fullscreen clients so it
// stays reachable, as tall as what it holds rather than full height. Everything
// in it comes from one process -- `garage-metrics --stream`, one JSON object per
// line at 1 Hz -- which is the same collector the bar renders its strips from, so
// the panel and the bar cannot disagree about what the machine is doing.
//
// The stream's lifetime is this window's lifetime. Nothing here polls when the
// panel is closed: `running` is bound to `visible`, so the collector is spawned
// as the panel maps and reaped as it unmaps, and the destruction guard at the
// foot of the file covers the case where the window is destroyed outright (which
// is the normal one -- the shell keeps its palettes in LazyLoaders).
PanelWindow {
    id: monitor

    // The monitor this was asked for, resolved by the shell at the moment of the
    // click and held as a name. Same contract as every other palette.
    required property string targetScreenName

    signal dismissed()

    // Set while the panel must not be taken down by an ambient dismissal -- the
    // shape the shell needs for the screenshot flow, where the panel being
    // photographed has to survive the pill and the capture. Honoured here for
    // Escape; the shell binds the shared DismissCatcher's `armed` to it for the
    // click outside, the way it already does for a capture in flight.
    property bool holdOpen: false

    readonly property int contentMargin: 12

    // The rolling window, in samples. 120 at the stream's 1 Hz is two minutes;
    // the seed the collector hands over is 120 samples of the bar's own 2s
    // history, so a freshly opened panel shows four minutes and then walks down
    // to two as live samples push the seeded ones off the left. That seam is
    // visible only as the graph speeding up, and the alternative -- discarding
    // the seed to keep one uniform time base -- is a panel that opens empty.
    readonly property int capacity: 120

    // The collector's LOG_CEILING_MIB. Throughput is plotted on a log curve
    // because idle is 0.01 MiB/s and an NVMe burst is 3000, and the seeded
    // history was written through the Python side of this constant: a live sample
    // scaled against a different ceiling would step at the seam between the two.
    readonly property real logCeilingMib: 2048
    readonly property real mib: 1048576

    // The last full snapshot, held whole rather than unpacked into a property per
    // number. Every reading below is a binding on this, so one assignment per
    // second updates the entire panel and there is no half-updated frame where
    // the CPU is from this second and the GPU from the last.
    property var latest: null

    // What the collector said went wrong, if it did. The stream reports its own
    // errors as JSON rather than dying, so this is a line in the header rather
    // than an empty panel.
    property string streamError: ""

    // One series per graph, oldest sample first. Reassigned rather than mutated
    // on every push: a mutated array does not notify, and the charts are bound to
    // these.
    property var cpuHistory: []
    property var tempHistory: []
    property var memoryHistory: []
    property var networkDownHistory: []
    property var networkUpHistory: []
    property var diskHistory: []
    property var gpuHistory: []
    property var gpuVramHistory: []

    // Per-core load, live only. There is no history for it -- the bar never
    // stored one, and 32 sparklines is not a thing to put in a panel; the bars
    // below are the instantaneous picture the overall graph does not show, which
    // is one core pinned while the average looks idle.
    property var coreValues: []

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === monitor.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    // -- Numbers -------------------------------------------------------------

    // Every reading in the stream is nullable: a box with no PSI has no memory
    // pressure, an Intel card reports no VRAM in use, a machine with no k10temp
    // has no package temperature. Nothing here may turn a missing reading into a
    // zero, so this is what stands between the JSON and every label below.
    function number(value) {
        if (value === null || value === undefined)
            return NaN;
        const parsed = Number(value);
        return isFinite(parsed) ? parsed : NaN;
    }

    function percent(value) {
        const parsed = monitor.number(value);
        return isNaN(parsed) ? NaN : Math.max(0, Math.min(100, parsed));
    }

    // The collector's log_scale. Written with log(1 + x) rather than log1p so it
    // leans on nothing beyond the QML engine's baseline Math; the two agree to
    // well past the precision a 2px-per-sample graph can show.
    function logScale(mibPerSecond) {
        const value = monitor.number(mibPerSecond);
        if (isNaN(value) || value <= 0)
            return 0;
        return Math.min(100,
            Math.log(1 + value) / Math.log(1 + monitor.logCeilingMib) * 100);
    }

    function formatBytes(bytes) {
        const value = monitor.number(bytes);
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
        const value = monitor.number(bytesPerSecond);
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

    // -- Stream --------------------------------------------------------------

    function pushPoint(series, value) {
        const list = Array.isArray(series) ? series : [];
        const number = Number(value);
        // Trimmed before the push rather than after, so the array is never
        // capacity + 1 long even for an instant.
        const next = list.slice(Math.max(0, list.length + 1 - monitor.capacity));
        next.push(isFinite(number) ? number : 0);
        return next;
    }

    // One series out of the seed object, trimmed to the window and sanitised.
    // An absent or empty key leaves the graph as it was, which for a freshly
    // opened panel means empty -- and an empty graph says "collecting…" rather
    // than drawing a flat line at zero it has no evidence for.
    function seedSeries(seed, key, current) {
        const list = seed ? seed[key] : null;
        if (!Array.isArray(list) || list.length === 0)
            return current;
        const values = [];
        for (let index = Math.max(0, list.length - monitor.capacity);
                index < list.length; ++index) {
            const value = Number(list[index]);
            values.push(isFinite(value) ? value : 0);
        }
        return values;
    }

    // The stream's first line: the bar's stored history, read by the collector
    // from ~/.local/state/garage/metrics/<widget>.json and handed over as flat
    // lists of the same 0..100 values the graphs plot.
    //
    // Read from the stream rather than from those files directly, which is the
    // one place this panel does not do what it was specified to do. The reason is
    // that the bar guards those files with an flock and writes them atomically,
    // and the collector's own reader already honours that; a FileView here would
    // be a second, unlocked reader of a file being rewritten every two seconds,
    // parsing six JSON documents in QML to arrive at exactly the lists the
    // process on the other end of this pipe has already assembled. Same data,
    // same source, one reader.
    function applySeed(seed) {
        monitor.cpuHistory = monitor.seedSeries(seed, "cpu", monitor.cpuHistory);
        monitor.tempHistory = monitor.seedSeries(seed, "temp", monitor.tempHistory);
        monitor.memoryHistory = monitor.seedSeries(seed, "memory", monitor.memoryHistory);
        monitor.networkDownHistory =
            monitor.seedSeries(seed, "network", monitor.networkDownHistory);
        monitor.networkUpHistory =
            monitor.seedSeries(seed, "network_up", monitor.networkUpHistory);
        monitor.diskHistory = monitor.seedSeries(seed, "disk", monitor.diskHistory);
        monitor.gpuHistory = monitor.seedSeries(seed, "gpu", monitor.gpuHistory);
        monitor.gpuVramHistory =
            monitor.seedSeries(seed, "gpu_vram", monitor.gpuVramHistory);
    }

    function applySnapshot(snapshot) {
        monitor.latest = snapshot;

        const cpu = snapshot.cpu || null;
        if (cpu !== null) {
            const load = monitor.percent(cpu.load);
            if (!isNaN(load))
                monitor.cpuHistory = monitor.pushPoint(monitor.cpuHistory, load);
            // The first snapshot after the collector primes its counters carries
            // no per-core figures, and a later one carries none if the core count
            // changed under it. Neither is a reason to blank the bars.
            if (Array.isArray(cpu.per_core) && cpu.per_core.length > 0)
                monitor.coreValues = cpu.per_core.slice();
        }

        // Nothing is pushed for a reading the machine does not have. A skipped
        // push leaves that graph shorter than its neighbours, which is correct:
        // the charts are independent, and a graph with no samples says so.
        const celsius = snapshot.temp ? monitor.number(snapshot.temp.cpu_c) : NaN;
        if (!isNaN(celsius))
            monitor.tempHistory = monitor.pushPoint(monitor.tempHistory,
                Math.max(0, Math.min(100, celsius)));

        const memory = snapshot.memory || null;
        if (memory !== null) {
            const total = monitor.number(memory.total);
            const used = monitor.number(memory.used);
            if (!isNaN(total) && total > 0 && !isNaN(used))
                monitor.memoryHistory = monitor.pushPoint(monitor.memoryHistory,
                    used / total * 100);
        }

        const network = snapshot.network || null;
        if (network !== null) {
            monitor.networkDownHistory = monitor.pushPoint(monitor.networkDownHistory,
                monitor.logScale(monitor.number(network.rx_bps) / monitor.mib));
            monitor.networkUpHistory = monitor.pushPoint(monitor.networkUpHistory,
                monitor.logScale(monitor.number(network.tx_bps) / monitor.mib));
        }

        const disk = snapshot.disk || null;
        if (disk !== null) {
            const read = monitor.number(disk.read_bps);
            const write = monitor.number(disk.write_bps);
            const total = (isNaN(read) ? 0 : read) + (isNaN(write) ? 0 : write);
            monitor.diskHistory = monitor.pushPoint(monitor.diskHistory,
                monitor.logScale(total / monitor.mib));
        }

        const gpu = monitor.primaryGpu(snapshot);
        if (gpu !== null) {
            const load = monitor.number(gpu.load);
            if (!isNaN(load))
                monitor.gpuHistory = monitor.pushPoint(monitor.gpuHistory,
                    Math.max(0, Math.min(100, load)));
            const vramTotal = monitor.number(gpu.vram_total);
            const vramUsed = monitor.number(gpu.vram_used);
            if (!isNaN(vramTotal) && vramTotal > 0 && !isNaN(vramUsed))
                monitor.gpuVramHistory = monitor.pushPoint(monitor.gpuVramHistory,
                    vramUsed / vramTotal * 100);
        }
    }

    // The discrete card if there is one: the collector sorts discrete first, and
    // it is the one whose load anybody opens this panel to look at.
    function primaryGpu(snapshot) {
        if (!snapshot || !Array.isArray(snapshot.gpus) || snapshot.gpus.length === 0)
            return null;
        return snapshot.gpus[0];
    }

    function consume(line) {
        const text = String(line).trim();
        if (text === "")
            return;

        let object = null;
        try {
            object = JSON.parse(text);
        } catch (error) {
            // A truncated line is not worth a log entry every second. The next
            // one is a whole second away and the graphs simply do not move.
            return;
        }
        if (object === null || typeof object !== "object")
            return;

        if (object.seed !== undefined) {
            monitor.applySeed(object.seed);
            return;
        }
        if (object.error !== undefined) {
            monitor.streamError = String(object.error);
            return;
        }

        monitor.streamError = "";
        monitor.applySnapshot(object);
    }

    // -- Readings --------------------------------------------------------------

    readonly property var cpuInfo: monitor.latest && monitor.latest.cpu
        ? monitor.latest.cpu : null
    readonly property var memoryInfo: monitor.latest && monitor.latest.memory
        ? monitor.latest.memory : null
    readonly property var networkInfo: monitor.latest && monitor.latest.network
        ? monitor.latest.network : null
    readonly property var diskInfo: monitor.latest && monitor.latest.disk
        ? monitor.latest.disk : null
    readonly property var gpuInfo: monitor.primaryGpu(monitor.latest)
    readonly property int gpuCount: monitor.latest
        && Array.isArray(monitor.latest.gpus) ? monitor.latest.gpus.length : 0

    readonly property real cpuLoad: monitor.cpuInfo
        ? monitor.percent(monitor.cpuInfo.load) : NaN
    readonly property string cpuLoadAverage: {
        const load = monitor.cpuInfo ? monitor.cpuInfo.loadavg : null;
        if (!Array.isArray(load) || load.length < 3)
            return "";
        return "load " + Number(load[0]).toFixed(2)
            + " " + Number(load[1]).toFixed(2)
            + " " + Number(load[2]).toFixed(2);
    }

    readonly property real cpuCelsius: monitor.latest && monitor.latest.temp
        ? monitor.number(monitor.latest.temp.cpu_c) : NaN
    readonly property string cpuSensor: {
        const temp = monitor.latest ? monitor.latest.temp : null;
        const label = temp ? String(temp.label || "").trim() : "";
        return label !== "" ? label : "unknown sensor";
    }

    readonly property real memoryPercent: {
        const total = monitor.memoryInfo ? monitor.number(monitor.memoryInfo.total) : NaN;
        const used = monitor.memoryInfo ? monitor.number(monitor.memoryInfo.used) : NaN;
        if (isNaN(total) || total <= 0 || isNaN(used))
            return NaN;
        return used / total * 100;
    }
    readonly property real memoryPressure: monitor.memoryInfo
        ? monitor.number(monitor.memoryInfo.pressure_some_avg10) : NaN

    readonly property real networkDown: monitor.networkInfo
        ? monitor.number(monitor.networkInfo.rx_bps) : NaN
    readonly property real networkUp: monitor.networkInfo
        ? monitor.number(monitor.networkInfo.tx_bps) : NaN

    readonly property real diskRead: monitor.diskInfo
        ? monitor.number(monitor.diskInfo.read_bps) : NaN
    readonly property real diskWrite: monitor.diskInfo
        ? monitor.number(monitor.diskInfo.write_bps) : NaN

    readonly property real gpuLoad: monitor.gpuInfo
        ? monitor.number(monitor.gpuInfo.load) : NaN

    // Never "utilization" for an Intel card: the collector labels that reading
    // "activity (freq proxy)" because it is derived from the clock rather than
    // measured, and the panel repeats the label it was given rather than
    // inventing a more confident one.
    readonly property string gpuLoadKind: {
        const kind = monitor.gpuInfo ? String(monitor.gpuInfo.load_kind || "").trim() : "";
        return kind !== "" ? kind : "load unavailable";
    }

    readonly property string gpuVramLabel: {
        if (monitor.gpuInfo === null)
            return "";
        const total = monitor.number(monitor.gpuInfo.vram_total);
        const used = monitor.number(monitor.gpuInfo.vram_used);
        if (!isNaN(used) && !isNaN(total) && total > 0)
            return "VRAM " + monitor.formatBytes(used) + " / " + monitor.formatBytes(total);
        if (!isNaN(total) && total > 0)
            return "VRAM " + monitor.formatBytes(total);
        return "VRAM n/a";
    }

    readonly property string statusText: {
        if (monitor.streamError !== "")
            return monitor.streamError;
        if (monitor.latest === null)
            return "connecting…";
        return "1 Hz";
    }

    // -- Surface ---------------------------------------------------------------

    screen: monitor.targetScreen()
    implicitWidth: 390
    // Sized to its content and clamped at both ends: at 1px because a layer
    // surface with no height is not a surface the compositor can show and the
    // column's implicit height is zero for the frame before its children are laid
    // out, and at the output's height because a panel taller than the screen
    // loses its bottom rows with no way to reach them. The Flickable below is
    // what makes the clamp survivable.
    implicitHeight: Math.max(1, Math.min(
        body.implicitHeight + monitor.contentMargin * 2, monitor.maxSurfaceHeight))
    color: "transparent"
    // OnDemand rather than Exclusive, as with the control centre: this is a panel
    // the user clicks into, not a modal, and an exclusive surface takes every
    // keystroke in the session while it is up. The compositor hands an on-demand
    // layer surface the keyboard as it maps, which is what makes the Escape at the
    // foot of this file heard without a click first.
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    // An overlay surface already begins below the bar's exclusive zone, and the
    // gutter is measured from there, so the room left is the output less the bar
    // and both gutters. A layer surface cannot ask how tall the bar is, so it is
    // taken at a generous 84px; whatever that allowance is wrong by is absorbed
    // by the Flickable rather than shown as a cut-off row.
    readonly property real maxSurfaceHeight: {
        const target = monitor.targetScreen();
        const available = target ? target.height : 1080;
        return Math.max(240, available - 84 - Theme.windowGutter * 2);
    }

    // Top and right only: the panel is as tall as its contents, so anchoring the
    // bottom as well would stretch it down the screen.
    anchors {
        top: true
        right: true
    }

    margins.top: Theme.windowGutter
    margins.right: Theme.windowGutter

    WlrLayershell.layer: WlrLayer.Overlay
    // Not yet in the compositor's glass layer_namespaces or its blur rules --
    // adding it there is part of wiring this panel up, and until it is there the
    // material is not drawn beneath this surface. Which is why the body below is
    // contentTint rather than bare glass: the panel is legible either way, and a
    // wall of small figures over live desktop content needs the body regardless.
    WlrLayershell.namespace: "garage-monitor"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    // The collector. One process, spawned as this window maps and reaped as it
    // unmaps: `running` is bound to `visible`, so a closed panel costs nothing
    // and there is no polling behind a surface nobody is looking at.
    //
    // The collector notices its own end of this too -- it exits on a broken pipe
    // rather than writing into a closed one.
    Process {
        id: stream
        running: monitor.visible
        command: [Quickshell.env("HOME") + "/.local/bin/garage-metrics", "--stream"]

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: data => monitor.consume(data)
        }

        // The exit code alone; exited() also carries a QProcess::ExitStatus this
        // has no use for. qmllint cannot resolve that second parameter's C++ enum
        // from Quickshell's qmltypes and says so about the handler either way --
        // it is a lint-side gap rather than anything this file can fix.
        onExited: exitCode => {
            // Only worth saying while the panel is up and on its way somewhere.
            // Closing the panel is what ends this process -- and the kill that
            // does it reports a non-zero code -- so without the two guards every
            // dismissal would write an error into a header on its way out.
            if (!monitor.closing && monitor.visible && exitCode !== 0)
                monitor.streamError = "collector exited (" + exitCode + ")";
        }
    }

    // Set for the last instant of this window's life. The window is normally
    // destroyed rather than hidden -- the shell keeps its palettes in LazyLoaders
    // -- and a destroyed window notifies nothing, so the binding above never sees
    // the close; clearing `running` by hand is what makes sure the collector goes
    // with the panel instead of being left streaming into a pipe with no reader,
    // and the flag is what stops the kill that follows from being reported as a
    // failure.
    property bool closing: false

    Component.onDestruction: {
        monitor.closing = true;
        stream.running = false;
    }

    // One metric's block: the well every section is drawn in, so the six of them
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

    ContinuousRectangle {
        id: panel
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        color: Theme.panel
        borderWidth: 1
        borderColor: Theme.frameOuter

        NumberAnimation on opacity {
            from: 0
            to: 1
            duration: 130
            easing.type: Easing.OutCubic
        }

        // The content body, one inset pixel in so the frame stays the outermost
        // thing drawn. Every other glass palette in the shell leaves this to the
        // material; this one carries its own because a panel of 10px figures over
        // whatever is playing behind it has to hold its contrast itself.
        ContinuousRectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Theme.insetRadius(panel.radius, 1)
            power: Theme.cornerPower
            color: Theme.contentTint
            borderWidth: 1
            borderColor: Theme.frameInner
        }

        // The panel eats the clicks that land in the gaps between its sections
        // rather than leaving them unhandled.
        MouseArea {
            anchors.fill: parent
        }

        // Scrolls only when the clamp above has actually bitten, so on a normal
        // display this is an ordinary column that happens to be inside a
        // Flickable.
        Flickable {
            id: scroll
            anchors.fill: parent
            anchors.margins: monitor.contentMargin
            clip: true
            contentWidth: scroll.width
            contentHeight: body.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            interactive: scroll.contentHeight > scroll.height

            ColumnLayout {
                id: body
                // The Flickable's width rather than the content item's: the
                // content item is as wide as contentWidth, which is bound to this
                // width in turn, and going through it would be a longer way round
                // to the same number.
                width: scroll.width
                spacing: 9

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    Text {
                        text: "Activity"
                        color: Theme.text
                        font.family: Theme.sans
                        font.pixelSize: 17
                        font.weight: Font.DemiBold
                        renderType: Text.NativeRendering
                    }

                    Item { Layout.fillWidth: true }

                    Text {
                        Layout.maximumWidth: 180
                        text: monitor.statusText
                        color: monitor.streamError !== ""
                            ? Theme.text : Theme.textDisabled
                        font.family: Theme.sans
                        font.pixelSize: 11
                        elide: Text.ElideRight
                        horizontalAlignment: Text.AlignRight
                        renderType: Text.NativeRendering
                    }
                }

                MenuSeparator {}

                // -- CPU ---------------------------------------------------
                Section {
                    MetricGraphRow {
                        label: "CPU"
                        value: isNaN(monitor.cpuLoad)
                            ? "--" : monitor.cpuLoad.toFixed(0) + "%"
                        extra: monitor.cpuLoadAverage
                        points: monitor.cpuHistory
                        active: !isNaN(monitor.cpuLoad) && monitor.cpuLoad >= 25
                        graphHeight: 38
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
                                model: monitor.coreValues

                                delegate: Item {
                                    required property var modelData
                                    // A core with no reading is drawn as idle
                                    // rather than as a gap: the row's job is to
                                    // show one core pinned against the rest, and a
                                    // missing slot would read as a missing core.
                                    readonly property real load: {
                                        const value = monitor.percent(modelData);
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
                                        color: Theme.accent
                                        opacity: 0.85
                                    }
                                }
                            }
                        }
                    }
                }

                // -- CPU temperature ---------------------------------------
                Section {
                    MetricGraphRow {
                        label: "CPU Temperature"
                        value: isNaN(monitor.cpuCelsius)
                            ? "n/a" : monitor.cpuCelsius.toFixed(0) + "°C"
                        points: monitor.tempHistory
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
                        points: monitor.memoryHistory
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
                    MetricGraphRow {
                        label: "Network"
                        value: "↓ " + monitor.formatRate(monitor.networkDown)
                        extra: "↑ " + monitor.formatRate(monitor.networkUp)
                        points: monitor.networkDownHistory
                        // Up as the second line, no fill and a dimmer stroke: it
                        // shares the axis with down and is the subordinate of the
                        // two on a desktop.
                        secondaryPoints: monitor.networkUpHistory
                        active: !isNaN(monitor.networkDown) && !isNaN(monitor.networkUp)
                            && (monitor.networkDown + monitor.networkUp) / monitor.mib >= 0.1
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
                    MetricGraphRow {
                        label: "Disk"
                        value: "r " + monitor.formatRate(monitor.diskRead)
                        extra: "w " + monitor.formatRate(monitor.diskWrite)
                        // Read and write are graphed as one line rather than two.
                        // That is what the bar stores and therefore what the seed
                        // can restore: splitting the line would mean opening this
                        // section empty every time, and the two figures above are
                        // where the split actually gets read.
                        points: monitor.diskHistory
                        active: !isNaN(monitor.diskRead) && !isNaN(monitor.diskWrite)
                            && (monitor.diskRead + monitor.diskWrite) / monitor.mib >= 1
                        detail: {
                            const device = monitor.diskInfo
                                ? String(monitor.diskInfo.device || "").trim() : "";
                            const read = isNaN(monitor.diskRead) ? 0 : monitor.diskRead;
                            const write = isNaN(monitor.diskWrite) ? 0 : monitor.diskWrite;
                            return (device !== "" ? device : "no block device")
                                + " · total " + monitor.formatRate(read + write)
                                + " · log scale to 2 GiB/s";
                        }
                    }
                }

                // -- GPU ---------------------------------------------------
                Section {
                    MetricGraphRow {
                        label: "GPU"
                        value: isNaN(monitor.gpuLoad)
                            ? "n/a" : monitor.gpuLoad.toFixed(0) + "%"
                        extra: monitor.gpuVramLabel
                        points: monitor.gpuHistory
                        secondaryPoints: monitor.gpuVramHistory
                        active: !isNaN(monitor.gpuLoad) && monitor.gpuLoad >= 10
                        detail: {
                            if (monitor.gpuInfo === null)
                                return monitor.latest === null ? "" : "no GPU found";
                            const parts = [
                                String(monitor.gpuInfo.name || "GPU").trim(),
                                monitor.gpuLoadKind
                            ];
                            const celsius = monitor.number(monitor.gpuInfo.temp_c);
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
            }
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: {
            if (!monitor.holdOpen)
                monitor.dismissed();
        }
    }
}
