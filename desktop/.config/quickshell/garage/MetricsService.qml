pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

// The metrics collector: one `garage-metrics --stream` process for the whole shell.
//
// The bar's strips and MonitorPalette both read from this single stream instead of each
// spawning their own -- one process, one JSON line per second, pushed to every reader.
// The first line of the stream is the seed: the collector's own stored history for every
// widget, so a strip drawn at login starts full rather than filling over a minute.
Singleton {
    id: metrics

    // Samples kept per series. The strips are ~60 px wide, one point per px; the
    // monitor panel's graphs are the same shape and share these arrays.
    readonly property int capacity: 120

    // The newest snapshot, whole, for label reads that want fields rather than series.
    property var latest: null

    // Set while the collector reports a failed sensor; cleared by the next good frame.
    property string streamError: ""

    // Series keyed as the monitor panel has always named them. Each is a plain array
    // replaced on push, never mutated, so bindings fire and cached references stay safe.
    property var cpuHistory: []
    property var tempHistory: []
    property var memoryHistory: []
    property var networkDownHistory: []
    property var networkUpHistory: []
    property var diskHistory: []
    property var gpuHistory: []

    Process {
        id: stream
        running: true
        command: [Quickshell.env("HOME") + "/.local/bin/garage-metrics", "--stream"]

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: data => metrics.consume(data)
        }

        onExited: exitCode => {
            if (exitCode !== 0)
                metrics.streamError = "collector exited (" + exitCode + ")";
            // A dead collector restarts in place; the seed re-backfills the series so
            // nothing but the seconds it was down goes missing.
            restartTimer.restart();
        }
    }

    property Timer restartTimer: Timer {
        interval: 2000
        onTriggered: stream.running = true
    }

    function number(value) {
        if (value === null || value === undefined)
            return NaN;
        const parsed = Number(value);
        return isFinite(parsed) ? parsed : NaN;
    }

    function percent(value) {
        const parsed = number(value);
        return isNaN(parsed) ? NaN : Math.max(0, Math.min(100, parsed));
    }

    // The collector's log_scale, written without log1p exactly as the monitor panel
    // writes its own copy.
    readonly property real logCeilingMib: 100
    readonly property real mib: 1048576

    function logScale(mibPerSecond) {
        const value = number(mibPerSecond);
        if (isNaN(value) || value <= 0)
            return 0;
        return Math.min(100,
            Math.log(1 + value) / Math.log(1 + logCeilingMib) * 100);
    }

    function pushPoint(series, value) {
        const list = Array.isArray(series) ? series : [];
        const next = list.slice(Math.max(0, list.length + 1 - capacity));
        next.push(isFinite(value) ? value : 0);
        return next;
    }

    function seedSeries(seed, key) {
        const list = seed ? seed[key] : null;
        if (!Array.isArray(list) || list.length === 0)
            return null;
        const values = [];
        const start = Math.max(0, list.length - capacity);
        for (let index = start; index < list.length; ++index)
            values.push(isFinite(Number(list[index])) ? Number(list[index]) : 0);
        return values;
    }

    function consume(line) {
        const text = String(line).trim();
        if (text === "")
            return;
        let object = null;
        try {
            object = JSON.parse(text);
        } catch (error) {
            // One truncated line costs nothing; the next is a second away.
            return;
        }
        if (object === null || typeof object !== "object")
            return;

        if (object.seed !== undefined) {
            applySeed(object.seed);
            return;
        }
        if (object.error !== undefined) {
            streamError = String(object.error);
            return;
        }
        streamError = "";
        latest = object;
        applySnapshot(object);
    }

    function applySeed(seed) {
        const cpu = seedSeries(seed, "cpu");
        if (cpu !== null)
            cpuHistory = cpu;
        const temp = seedSeries(seed, "temp");
        if (temp !== null)
            tempHistory = temp;
        const memory = seedSeries(seed, "memory");
        if (memory !== null)
            memoryHistory = memory;
        const down = seedSeries(seed, "network");
        if (down !== null)
            networkDownHistory = down;
        const up = seedSeries(seed, "network_up");
        if (up !== null)
            networkUpHistory = up;
        const disk = seedSeries(seed, "disk");
        if (disk !== null)
            diskHistory = disk;
        const gpu = seedSeries(seed, "gpu");
        if (gpu !== null)
            gpuHistory = gpu;
    }

    function applySnapshot(snapshot) {
        const cpu = snapshot.cpu || null;
        if (cpu !== null) {
            const load = percent(cpu.load);
            if (!isNaN(load))
                cpuHistory = pushPoint(cpuHistory, load);
        }
        const celsius = snapshot.temp ? number(snapshot.temp.cpu_c) : NaN;
        if (!isNaN(celsius))
            tempHistory = pushPoint(tempHistory,
                Math.max(0, Math.min(100, celsius)));
        const memory = snapshot.memory || null;
        if (memory !== null) {
            const total = number(memory.total);
            const used = number(memory.used);
            if (!isNaN(total) && total > 0 && !isNaN(used))
                memoryHistory = pushPoint(memoryHistory, used / total * 100);
        }
        const network = snapshot.network || null;
        if (network !== null) {
            networkDownHistory = pushPoint(networkDownHistory,
                logScale(number(network.rx_bps) / mib));
            networkUpHistory = pushPoint(networkUpHistory,
                logScale(number(network.tx_bps) / mib));
        }
        const disk = snapshot.disk || null;
        if (disk !== null) {
            const read = number(disk.read_bps);
            const write = number(disk.write_bps);
            const total = (isNaN(read) ? 0 : read) + (isNaN(write) ? 0 : write);
            diskHistory = pushPoint(diskHistory, logScale(total / mib));
        }
        const gpus = Array.isArray(snapshot.gpus) ? snapshot.gpus : [];
        if (gpus.length > 0) {
            const load = number(gpus[0].load);
            if (!isNaN(load))
                gpuHistory = pushPoint(gpuHistory,
                    Math.max(0, Math.min(100, load)));
        }
    }

    // -- Strip labels --------------------------------------------------------

    function rateLabel(bytesPerSecond) {
        const value = number(bytesPerSecond);
        if (isNaN(value) || value < 0)
            return "--";
        if (value >= 1048576)
            return (value / 1048576).toFixed(1) + "M";
        if (value >= 1024)
            return (value / 1024).toFixed(0) + "K";
        return value.toFixed(0);
    }

    function tempLabel() {
        const celsius = latest && latest.temp ? number(latest.temp.cpu_c) : NaN;
        return isNaN(celsius) ? "--" : Math.round(celsius) + "°";
    }

    function memoryPercent() {
        const memory = latest ? latest.memory : null;
        if (!memory)
            return NaN;
        const total = number(memory.total);
        const used = number(memory.used);
        return !isNaN(total) && total > 0 && !isNaN(used) ? used / total * 100 : NaN;
    }

    function primaryGpuLoad() {
        const gpus = latest && Array.isArray(latest.gpus) ? latest.gpus : [];
        if (gpus.length === 0)
            return NaN;
        return percent(gpus[0].load);
    }

    // -- Strip accessors -----------------------------------------------------

    function seriesFor(name) {
        if (name === "cpu")
            return cpuHistory;
        if (name === "memory")
            return memoryHistory;
        if (name === "network")
            return networkDownHistory;
        if (name === "temp")
            return tempHistory;
        if (name === "disk")
            return diskHistory;
        if (name === "gpu")
            return gpuHistory;
        return [];
    }

    // The one figure the strip prints beside its graph, compact enough for the
    // widths the old layout table budgeted.
    function labelFor(name) {
        if (name === "cpu") {
            const load = latest && latest.cpu ? percent(latest.cpu.load) : NaN;
            return isNaN(load) ? "--" : Math.round(load) + "%";
        }
        if (name === "memory") {
            const share = memoryPercent();
            return isNaN(share) ? "--" : Math.round(share) + "%";
        }
        if (name === "network")
            return networkLabel();
        if (name === "temp")
            return tempLabel();
        if (name === "disk")
            return diskLabel();
        if (name === "gpu") {
            const load = primaryGpuLoad();
            return isNaN(load) ? "--" : Math.round(load) + "%";
        }
        return "--";
    }

    function networkLabel() {
        const net = latest ? latest.network : null;
        if (!net)
            return "--";
        return "\u2193" + rateLabel(net.rx_bps) + " \u2191" + rateLabel(net.tx_bps);
    }

    function downRate() {
        const net = latest ? latest.network : null;
        return net ? number(net.rx_bps) : NaN;
    }

    function upRate() {
        const net = latest ? latest.network : null;
        return net ? number(net.tx_bps) : NaN;
    }

    function diskLabel() {
        const disk = latest ? latest.disk : null;
        if (!disk)
            return "--";
        const read = number(disk.read_bps);
        const write = number(disk.write_bps);
        const total = (isNaN(read) ? 0 : read) + (isNaN(write) ? 0 : write);
        return rateLabel(total) + "/s";
    }

    // The tooltip: the same lines the old SVG strips carried in theirs.
    function tipFor(name) {
        if (name === "cpu") {
            const cpu = latest ? latest.cpu : null;
            if (!cpu)
                return "CPU";
            const average = Array.isArray(cpu.loadavg) && cpu.loadavg.length >= 3
                ? "load " + Number(cpu.loadavg[0]).toFixed(2) + " "
                    + Number(cpu.loadavg[1]).toFixed(2) + " "
                    + Number(cpu.loadavg[2]).toFixed(2) : "";
            const load = percent(cpu.load);
            return "CPU " + (isNaN(load) ? "--" : Math.round(load) + "%")
                + (average !== "" ? "\n" + average : "");
        }
        if (name === "memory") {
            const memory = latest ? latest.memory : null;
            if (!memory)
                return "Memory";
            const used = number(memory.used);
            const total = number(memory.total);
            const gib = value => isNaN(value) ? "?" : (value / 1073741824).toFixed(1);
            return "Memory " + (isNaN(memoryPercent())
                ? "--" : Math.round(memoryPercent()) + "%")
                + "\n" + gib(used) + " / " + gib(total) + " GiB";
        }
        if (name === "network") {
            const net = latest ? latest.network : null;
            if (!net)
                return "Network";
            return String(net.iface || "Network")
                + "\n\u2193 " + rateLabel(net.rx_bps) + "/s"
                + "   \u2191 " + rateLabel(net.tx_bps) + "/s";
        }
        if (name === "temp") {
            const temp = latest ? latest.temp : null;
            const celsius = temp ? number(temp.cpu_c) : NaN;
            return "Temperature\n" + (isNaN(celsius)
                ? "--" : Math.round(celsius) + " \u00b0C"
                    + (temp.label ? " (" + temp.label + ")" : ""));
        }
        if (name === "disk") {
            const disk = latest ? latest.disk : null;
            if (!disk)
                return "Disk";
            return "Disk " + String(disk.device || "")
                + "\n\u2193 read " + diskLabel() + "\nwrite "
                + rateLabel(disk.write_bps) + "/s";
        }
        if (name === "gpu") {
            const gpus = latest && Array.isArray(latest.gpus) ? latest.gpus : [];
            const load = primaryGpuLoad();
            return gpus.length > 0
                ? "GPU " + String(gpus[0].name || "") + "\n"
                    + (isNaN(load) ? "--" : Math.round(load) + "%")
                : "GPU";
        }
        return name;
    }
}
