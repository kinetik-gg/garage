pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

// The metrics collector: one `garage-metrics --stream` process, refcounted.
//
// The stream runs only while something is reading it -- the system panel calls
// acquire() as it opens and release() as it closes, and the process follows the
// count. Nothing polls behind a popup nobody has open: the collector persists
// its own rolling history and hands it back as the seed line on the next spawn,
// so a reader that comes back after an hour still opens with full graphs.
// The first line of the stream is that seed; everything after is one JSON
// snapshot per second, pushed to every reader.
Singleton {
    id: metrics

    // How many open surfaces are reading the stream right now. The process's
    // lifetime is this count's: acquire on open, release on close, and the
    // collector is reaped the moment the last reader lets go.
    property int refs: 0

    function acquire() {
        refs += 1;
    }

    function release() {
        if (refs > 0)
            refs -= 1;
    }

    // Samples kept per series. The panel's graphs are ~2px per sample; 120 at
    // the stream's 1 Hz is two minutes, and the seed restores the same window.
    readonly property int capacity: 120

    // The newest snapshot, whole, for label reads that want fields rather than series.
    property var latest: null

    // What went wrong, if anything, in the shape the system icon binds: `error`
    // is the collector's own report (or its exit), cleared by the next good
    // frame; `available` drops only when a spawn dies without ever producing a
    // line -- the missing-binary case -- so a degraded icon and a transient
    // sensor failure read differently.
    property string error: ""
    property bool available: true

    // Series keyed as the system panel has always named them. Each is a plain array
    // replaced on push, never mutated, so bindings fire and cached references stay safe.
    property var cpuHistory: []
    property var tempHistory: []
    property var memoryHistory: []
    property var networkDownHistory: []
    property var networkUpHistory: []
    property var diskHistory: []
    property var gpuHistory: []
    property var gpuVramHistory: []

    // Per-core load, live only: the collector stores no history for it, and the
    // panel's core bars are an instantaneous picture by design.
    property var coreValues: []

    // Set while a line has arrived since the last spawn. What separates a
    // collector that started and then failed from one that never existed.
    property bool sawData: false

    // Down while the respawn timer is pending, so the running binding stays a
    // binding: an exit flips this true (running drops), the timer flips it back
    // (running re-evaluates and respawns) -- no imperative assignment that
    // would sever the refcount from the process.
    property bool cooldown: false

    // True after the last reader releases the collector. Process reports the
    // resulting termination through onExited too; keeping that exit distinct
    // prevents a normal panel dismissal from becoming a red system warning.
    property bool stopping: false

    onRefsChanged: {
        if (refs !== 0)
            return;
        stopping = true;
        restartTimer.stop();
        cooldown = false;
    }

    Process {
        id: stream
        running: metrics.refs > 0 && !metrics.cooldown
        command: [GaragePaths.metrics, "--stream"]

        onStarted: {
            metrics.sawData = false;
            metrics.stopping = false;
        }

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: data => metrics.consume(data)
        }

        onExited: exitCode => {
            // A release kills the process and the kill reports non-zero; only
            // an exit with a reader still waiting is worth reporting or
            // retrying. The seed re-backfills the series on respawn, so
            // nothing but the seconds it was down goes missing.
            if (metrics.stopping) {
                // A reader may have reacquired while the intentional stop was
                // still being delivered. Retry that demand after this old
                // process has fully gone away.
                if (metrics.refs > 0) {
                    metrics.cooldown = true;
                    restartTimer.restart();
                }
                return;
            }
            if (metrics.refs === 0)
                return;
            if (!metrics.sawData)
                metrics.available = false;
            metrics.error = exitCode !== 0
                ? "collector exited (" + exitCode + ")"
                : metrics.sawData ? "collector stopped"
                    : "collector exited before reporting data";
            metrics.cooldown = true;
            restartTimer.restart();
        }
    }

    property Timer restartTimer: Timer {
        interval: 2000
        onTriggered: metrics.cooldown = false
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

    // The collector's LOG_CEILING_MIB, written without log1p exactly as the
    // Python side writes it. One ceiling for every reader: the seeded history
    // was scaled through this constant on the way to disk, and a live sample
    // scaled against a different one would step at the seam between the two.
    readonly property real logCeilingMib: 2048
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
        } catch (parseError) {
            // One truncated line costs nothing; the next is a second away.
            return;
        }
        if (object === null || typeof object !== "object")
            return;

        sawData = true;
        available = true;

        if (object.seed !== undefined) {
            error = "";
            applySeed(object.seed);
            return;
        }
        if (object.error !== undefined) {
            error = String(object.error);
            return;
        }
        error = "";
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
        const vram = seedSeries(seed, "gpu_vram");
        if (vram !== null)
            gpuVramHistory = vram;
    }

    function applySnapshot(snapshot) {
        const cpu = snapshot.cpu || null;
        if (cpu !== null) {
            const load = percent(cpu.load);
            if (!isNaN(load))
                cpuHistory = pushPoint(cpuHistory, load);
            // The first snapshot after the collector primes its counters
            // carries no per-core figures. Not a reason to blank the bars.
            if (Array.isArray(cpu.per_core) && cpu.per_core.length > 0)
                coreValues = cpu.per_core.slice();
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
        const gpu = primaryGpu(snapshot);
        if (gpu !== null) {
            const load = number(gpu.load);
            if (!isNaN(load))
                gpuHistory = pushPoint(gpuHistory,
                    Math.max(0, Math.min(100, load)));
            const vramTotal = number(gpu.vram_total);
            const vramUsed = number(gpu.vram_used);
            if (!isNaN(vramTotal) && vramTotal > 0 && !isNaN(vramUsed))
                gpuVramHistory = pushPoint(gpuVramHistory,
                    vramUsed / vramTotal * 100);
        }
    }

    // The discrete card if there is one: the collector sorts discrete first, and
    // it is the one whose load anybody opens the panel to look at.
    function primaryGpu(snapshot) {
        if (!snapshot || !Array.isArray(snapshot.gpus) || snapshot.gpus.length === 0)
            return null;
        return snapshot.gpus[0];
    }

    // -- Labels ---------------------------------------------------------------

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
        const gpu = primaryGpu(latest);
        return gpu !== null ? percent(gpu.load) : NaN;
    }

    // -- Accessors ------------------------------------------------------------

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

    // The one figure worth printing beside a graph, compact enough for a strip.
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
        return "↓" + rateLabel(net.rx_bps) + " ↑" + rateLabel(net.tx_bps);
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
                + "\n↓ " + rateLabel(net.rx_bps) + "/s"
                + "   ↑ " + rateLabel(net.tx_bps) + "/s";
        }
        if (name === "temp") {
            const temp = latest ? latest.temp : null;
            const celsius = temp ? number(temp.cpu_c) : NaN;
            return "Temperature\n" + (isNaN(celsius)
                ? "--" : Math.round(celsius) + " °C"
                    + (temp.label ? " (" + temp.label + ")" : ""));
        }
        if (name === "disk") {
            const disk = latest ? latest.disk : null;
            if (!disk)
                return "Disk";
            return "Disk " + String(disk.device || "")
                + "\n↓ read " + diskLabel() + "\nwrite "
                + rateLabel(disk.write_bps) + "/s";
        }
        if (name === "gpu") {
            const gpu = primaryGpu(latest);
            const load = primaryGpuLoad();
            return gpu !== null
                ? "GPU " + String(gpu.name || "") + "\n"
                    + (isNaN(load) ? "--" : Math.round(load) + "%")
                : "GPU";
        }
        return name;
    }
}
