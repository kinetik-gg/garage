import Quickshell
import Quickshell.Io
import QtQuick

// The bar spectrum drawn under MediaPalette's transport. cava has no QML
// binding of its own, so this drives it as a subprocess: a generated raw-
// format config is written to $XDG_RUNTIME_DIR and handed to `cava -p`,
// which streams ascii frames back over stdout that SplitParser cuts into
// lines and this file turns into bar heights.
//
// The generated config is mono and asks cava for half as many bars as are
// drawn: the row here mirrors that single half outward from the centre, low
// frequencies together in the middle and highs at the two ends, rather than
// asking cava's own `channels = stereo` output to do the mirroring itself.
// Halving here rather than there keeps the bar count this file asks cava
// for equal to the bar count it actually draws on each side, so a frame
// with the wrong number of values is a bug in this file rather than a
// mismatch between two independent halving decisions.
Item {
    id: visualizer

    // Bound by the parent to the palette's own visibility. cava has nothing
    // useful to draw when nobody can see it, and a hidden panel is not a
    // reason to keep a capture process open on the default sink's monitor.
    property bool running: false
    property color barColor: Theme.accent

    readonly property real barWidth: 3
    readonly property real barGap: 2

    // Bars per side, fitted to the component's own width. cava is asked for
    // exactly this many; twice that many are drawn.
    readonly property int halfBars: Math.max(4,
        Math.floor(visualizer.width / (2 * (visualizer.barWidth + visualizer.barGap))))

    readonly property int totalBars: visualizer.halfBars * 2

    readonly property real barsWidth: visualizer.totalBars * visualizer.barWidth
        + Math.max(0, visualizer.totalBars - 1) * visualizer.barGap

    readonly property real barsOrigin: (visualizer.width - visualizer.barsWidth) / 2

    // One frame, 0..1 per bar, low frequency first. Replaced wholesale on
    // every line from cava rather than mutated in place: the Repeater below
    // reads through levelForBar() on every delegate, and a mutated array
    // does not notify either of them.
    property var levels: []

    function levelAt(index) {
        return index >= 0 && index < visualizer.levels.length
            ? visualizer.levels[index] : 0;
    }

    // Bar `index` in the drawn, mirrored row back to its position in the one
    // half cava actually renders: the left side counts down to the centre,
    // the right side counts up from it, and index halfBars - 1 on the left
    // is the same source bar as index 0 on the right -- both are the lowest
    // frequency, which is what makes the seam in the middle read as one
    // continuous shape rather than two spectra glued together.
    function levelForBar(index) {
        const half = visualizer.halfBars;
        const sourceIndex = index < half ? (half - 1 - index) : (index - half);
        return visualizer.levelAt(sourceIndex);
    }

    readonly property string configPath:
        (Quickshell.env("XDG_RUNTIME_DIR") || "/tmp") + "/garage-cava.conf"

    // Raw ascii output, one line per frame: `v1;v2;...;vN;` (a trailing
    // delimiter, no leading one), values 0-100. Verified standalone against
    // this exact config: `cava -p` on it produces one line every ~33 ms with
    // as many semicolon-separated values as `bars` asks for.
    function configText(bars) {
        return "[general]\n"
            + "framerate = 30\n"
            + "bars = " + bars + "\n"
            + "autosens = 1\n"
            + "sensitivity = 100\n"
            + "\n"
            + "[input]\n"
            + "source = auto\n"
            + "\n"
            + "[output]\n"
            + "method = raw\n"
            + "raw_target = /dev/stdout\n"
            + "data_format = ascii\n"
            + "ascii_max_range = 100\n"
            + "bar_delimiter = 59\n"
            + "frame_delimiter = 10\n"
            + "channels = mono\n"
            + "\n"
            + "[smoothing]\n"
            + "noise_reduction = 77\n"
            + "monstercat = 0\n";
    }

    // The config has to exist on disk before `exec cava` can read it -- `-p`
    // is the only way cava takes one, there is no stdin form -- so the
    // heredoc that writes it and the exec that becomes cava are one shell
    // command, issued fresh on every start rather than a config kept live
    // and re-read out from under a running process.
    //
    // `exec` rather than a plain trailing command: it replaces the shell
    // with cava in place, keeping the same pid, which is what lets stopping
    // this Process reach the real cava instead of leaving it behind as an
    // orphaned child of a shell that already exited.
    function start() {
        const bars = visualizer.halfBars;
        const path = visualizer.configPath;
        const script = "cat > " + JSON.stringify(path) + " <<'GARAGE_CAVA_CFG'\n"
            + visualizer.configText(bars)
            + "GARAGE_CAVA_CFG\n"
            + "exec cava -p " + JSON.stringify(path) + "\n";
        cavaProcess.command = ["sh", "-c", script];
        cavaProcess.running = true;
    }

    function stop() {
        cavaProcess.running = false;
    }

    onRunningChanged: {
        if (visualizer.running)
            visualizer.start();
        else
            visualizer.stop();
    }

    // The panel that hosts this does not get resized once mapped, so a live
    // change of halfBars is not expected in practice -- but if width ever
    // moved, the bar count cava was asked for would silently stop matching
    // what is drawn, so the pipeline is restarted onto the new count rather
    // than left stretching stale data across it.
    onHalfBarsChanged: {
        if (cavaProcess.running)
            visualizer.start();
    }

    // The hard guard the brief calls for: however this item goes away --
    // `running` flipped false, or the loader above MediaPalette destroying
    // it outright while still visible -- the process is stopped here too.
    // cava is exec'd into the shell's own pid (see start() above), so this
    // reaches the real cava process rather than orphaning it under a parent
    // that already exited.
    Component.onDestruction: {
        if (cavaProcess.running)
            cavaProcess.running = false;
    }

    Process {
        id: cavaProcess

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: data => {
                const text = String(data || "").trim();
                if (text === "")
                    return;
                const parts = text.split(";").filter(part => part !== "");
                const bars = visualizer.halfBars;
                const next = new Array(bars);
                for (let index = 0; index < bars; ++index) {
                    const raw = index < parts.length ? parseInt(parts[index], 10) : 0;
                    next[index] = Math.max(0, Math.min(1, (isNaN(raw) ? 0 : raw) / 100));
                }
                visualizer.levels = next;
            }
        }
    }

    Repeater {
        model: visualizer.totalBars

        Rectangle {
            id: bar
            required property int index

            readonly property real level: visualizer.levelForBar(bar.index)

            x: visualizer.barsOrigin + bar.index * (visualizer.barWidth + visualizer.barGap)
            y: visualizer.height - height
            width: visualizer.barWidth
            height: Math.max(2, bar.level * visualizer.height)
            radius: visualizer.barWidth / 2
            color: visualizer.barColor

            // The cava side already smooths (noise_reduction = 77 above);
            // this smooths the frame-to-frame jump on top of that so a
            // ~30 fps feed still reads as motion rather than a strobe.
            Behavior on height {
                NumberAnimation { duration: 90; easing.type: Easing.OutQuad }
            }
        }
    }
}
