pragma Singleton
import Quickshell
import Quickshell.Io
import Quickshell.Services.Pipewire
import QtQuick

// Shared first-party extension data: containers, SMB shares, microphone,
// AI usage, and volume.
//
// Containers, SMB and the mic arrive on one JSON line every five seconds from
// `garage-bar-probe stream` -- one process for the three probes, replacing the
// previous bar's three separate poll spawns. Volume reads live from PipeWire's default sink;
// nothing here polls.
Singleton {
    id: context

    // -- Probe state ---------------------------------------------------------

    property int containerCount: 0
    property var containerNames: []
    // null means "no engine answered": hide the chip rather than draw an empty one.
    property bool containersAvailable: false

    property bool smbAvailable: false
    property int smbExpected: 0
    property int smbConnected: 0
    property var smbMissingLabels: []

    property bool micAvailable: false
    property bool micRecording: false
    property var micDescriptions: []

    // False only when the probe stream died without ever answering -- the
    // missing-binary case -- so consumers can show degraded rather than
    // pretending three quiet probes.
    property bool probeAvailable: true
    property string probeError: ""
    property bool probeSawData: false

    // -- Derived state --------------------------------------------------------

    // Shared with the System extension and its detail panel.
    readonly property bool smbShort: smbAvailable && smbMissingLabels.length > 0

    // -- AI usage ------------------------------------------------------------

    // A rolling billing-window figure that moves over minutes, not seconds: the old
    // module refreshed it every five minutes and so does this.
    property string aiGlyph: ""
    property string aiTip: ""
    property bool aiStale: false
    // False when garage-ai-usage says tokscale is not installed -- a normal
    // state, distinct from a stale reading, and what the dedicated AI extension
    // keys its empty state on.
    property bool aiAvailable: false

    Process {
        id: aiProcess

        command: [GaragePaths.aiUsage, "--bar"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const payload = JSON.parse(text);
                    context.aiGlyph = String(payload.glyph || "");
                    context.aiTip = String(payload.tip || "");
                    context.aiStale = payload.stale === true;
                    context.aiAvailable = payload.available === true;
                } catch (error) {
                    // Absent tokscale is a normal state and prints an empty glyph.
                    context.aiGlyph = "";
                    context.aiTip = "";
                    context.aiAvailable = false;
                }
            }
        }
    }

    Timer {
        interval: 300000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: if (!aiProcess.running) aiProcess.running = true
    }

    Process {
        id: probeStream
        running: true
        command: [GaragePaths.barProbe, "stream"]

        onStarted: context.probeSawData = false

        stdout: SplitParser {
            splitMarker: "\n"
            onRead: data => context.consume(data)
        }

        onExited: exitCode => {
            if (exitCode !== 0) {
                containersAvailable = false;
                smbAvailable = false;
                micAvailable = false;
                probeError = "probe exited (" + exitCode + ")";
            }
            if (!probeSawData)
                probeAvailable = false;
            restartTimer.restart();
        }
    }

    property Timer restartTimer: Timer {
        interval: 2000
        onTriggered: probeStream.running = true
    }

    function consume(line) {
        const text = String(line).trim();
        if (text === "")
            return;
        let object = null;
        try {
            object = JSON.parse(text);
        } catch (error) {
            return;
        }
        if (object === null || typeof object !== "object")
            return;

        probeSawData = true;
        probeAvailable = true;
        probeError = "";

        const boxes = object.containers;
        if (boxes === null || boxes === undefined) {
            containersAvailable = false;
        } else {
            containersAvailable = true;
            containerCount = Number(boxes.running) || 0;
            containerNames = Array.isArray(boxes.names) ? boxes.names : [];
        }

        const shares = object.smb;
        if (shares === null || shares === undefined) {
            smbAvailable = false;
        } else {
            smbAvailable = true;
            smbExpected = Number(shares.expected) || 0;
            smbConnected = Number(shares.connected) || 0;
            smbMissingLabels = Array.isArray(shares.missing_labels)
                ? shares.missing_labels : [];
        }

        const microphone = object.mic;
        if (microphone === null || microphone === undefined) {
            micAvailable = false;
        } else {
            micAvailable = true;
            micRecording = microphone.recording === true;
            micDescriptions = Array.isArray(microphone.descriptions)
                ? microphone.descriptions : [];
        }
    }

    Component.onCompleted: {
        // The seed line lands within a tick of the process starting; until then every
        // chip stays hidden, which is the same first-frame contract the old modules had.
    }

    // -- Volume --------------------------------------------------------------

    // Nothing on a Pipewire node binds until something tracks it, so the tracker is
    // what turns the default sink into readable audio -- the control centre's rule.
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var sinkAudio: sink && sink.ready ? sink.audio : null
    readonly property real volume: sinkAudio ? sinkAudio.volume : 0
    readonly property bool muted: sinkAudio !== null && sinkAudio.muted

    PwObjectTracker { objects: context.sink ? [context.sink] : [] }

    function setVolume(value) {
        if (sinkAudio)
            sinkAudio.volume = Math.max(0, Math.min(1, value));
    }

    function toggleMuted() {
        if (sinkAudio)
            sinkAudio.muted = !sinkAudio.muted;
    }
}
