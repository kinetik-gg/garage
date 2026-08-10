import Quickshell
import Quickshell.Io
import QtQuick

// The spectrum drawn behind MediaPalette's transport. cava has no QML binding of
// its own, so this drives it as a subprocess: a generated raw-format config is
// written to $XDG_RUNTIME_DIR and handed to `cava -p`, which streams ascii
// frames back over stdout that SplitParser cuts into lines and this file turns
// into one point per frequency band.
//
// Drawn as a GraphChart, not as bars. The panel sits next to an activity monitor
// full of GraphCharts and this is the same kind of picture -- a series of
// normalised 0..100 values across a fixed frame -- so it is the same component,
// which also means it inherits the monochrome treatment (see `ink` in
// GraphChart) rather than carrying a second opinion about how a graph looks.
//
// The bar row this replaced mirrored one half of the spectrum outward from the
// centre, lows in the middle and highs at the two ends. Measured on real music
// that read as a narrow cluster of movement in the middle with dead space to
// either side, because the outer bands were the 5-10 kHz end where music has
// almost no energy: the mirror put the quiet bands where there was the most room
// for them. So the mirror is gone -- low frequency at the left edge, high at the
// right, one continuous line across the whole width -- and the band range is
// capped below cava's 10 kHz default so the bands that do exist are bands that
// move.
Item {
    id: visualizer

    // Bound by the parent to the palette's own visibility. cava has nothing
    // useful to draw when nobody can see it, and a hidden panel is not a
    // reason to keep a capture process open on the default sink's monitor.
    property bool running: false

    // Monochrome, like the activity graphs: the same foreground token, and the
    // GraphChart below separates its fill from its stroke by opacity alone. This
    // used to be `barColor`, defaulted from Theme.accent, and it was the accent
    // the owner asked out of the visualiser.
    property color ink: Theme.text

    // Frequency bands, which are the points of the line. Fixed rather than
    // fitted to the width the way the old bar row's count was: a line has no
    // per-point width to fit, and the count is now a statement about the audio
    // -- how finely the audible range is cut -- rather than about pixels. 48
    // across ~350px is a point every 7px, which the round joins draw as a
    // continuous curve, and it stays well inside cava's own floor of 43 Hz of
    // bandwidth per band over the range below.
    readonly property int bandCount: 48

    // The range the bands are cut from. cava's own default upper bound is
    // 10 kHz, and the top third of a spectrum drawn to it never moves for
    // music -- which is what left the old row's edges dead. 6 kHz still holds
    // every harmonic that carries a mix and spreads the energy that exists
    // across the whole width instead of the middle of it. The lower bound goes
    // under the default so a kick drum is a band rather than the edge of one.
    readonly property int lowerCutoffHz: 30
    readonly property int higherCutoffHz: 6000

    // One frame, already in GraphChart's 0..100 units, lowest frequency first.
    // Replaced wholesale on every line from cava rather than mutated in place:
    // a mutated array does not notify, so the path bound to it would go stale.
    property var levels: []

    readonly property string configPath:
        (Quickshell.env("XDG_RUNTIME_DIR") || "/tmp") + "/garage-cava.conf"

    // Raw ascii output, one line per frame: `v1;v2;...;vN;` (a trailing
    // delimiter, no leading one), values 0-100 -- which is GraphChart's range
    // exactly, so a frame needs clamping but no rescaling. Verified standalone
    // against this config: `cava -p` on it produces one line every ~33 ms with
    // as many semicolon-separated values as `bars` asks for.
    function configText(bars) {
        return "[general]\n"
            + "framerate = 30\n"
            + "bars = " + bars + "\n"
            + "autosens = 1\n"
            + "sensitivity = 100\n"
            + "lower_cutoff_freq = " + visualizer.lowerCutoffHz + "\n"
            + "higher_cutoff_freq = " + visualizer.higherCutoffHz + "\n"
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
        const path = visualizer.configPath;
        const script = "cat > " + JSON.stringify(path) + " <<'GARAGE_CAVA_CFG'\n"
            + visualizer.configText(visualizer.bandCount)
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
                const bands = visualizer.bandCount;
                const next = new Array(bands);
                for (let index = 0; index < bands; ++index) {
                    const raw = index < parts.length ? parseInt(parts[index], 10) : 0;
                    next[index] = Math.max(0, Math.min(100, isNaN(raw) ? 0 : raw));
                }
                visualizer.levels = next;
            }
        }
    }

    // Edge to edge: the line's x axis is the whole component, so whatever width
    // the palette gives this is the width the spectrum spans. There is no
    // centred inner row any more, which is the other half of the dead-edges fix.
    //
    // No midline and no baseline. In the monitor those two hairlines are the
    // frame a 0..100 reading is measured against; here the series is a spectrum
    // rather than a percentage of anything, and a fixed rule drawn across the
    // panel behind the artwork and the transport is furniture the media panel
    // has no use for. Same for the idle label: a silent sink is not an error to
    // report, it is a flat line, and before the first frame it is nothing.
    GraphChart {
        id: spectrum
        anchors.fill: parent
        points: visualizer.levels
        ink: visualizer.ink
        active: true
        midlineOpacity: 0
        baselineOpacity: 0
        idleLabel: ""
    }
}
