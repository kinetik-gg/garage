import Quickshell
import Quickshell.Io
import Quickshell.Services.Pipewire
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

// In-shell feedback for the hardware keys. The binds still own the mutations;
// their successful command sends only the event and focused output over IPC.
// Audio is then read from the shell's live PipeWire graph, while brightness is
// queried once after brightnessctl has completed its write.
Scope {
    id: osd

    property string kind: "output"
    property string targetScreenName: ""
    property bool windowVisible: false
    property bool presented: false
    property bool brightnessKnown: false
    property bool brightnessAvailable: false
    property real brightnessLevel: 0
    property bool brightnessQueryPending: false

    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var source: Pipewire.defaultAudioSource
    readonly property var sinkAudio: sink && sink.ready ? sink.audio : null
    readonly property var sourceAudio: source && source.ready ? source.audio : null
    readonly property var activeAudio: kind === "microphone" ? sourceAudio : sinkAudio
    readonly property bool audioAvailable: activeAudio !== null
    readonly property bool muted: kind !== "brightness"
        && audioAvailable && activeAudio.muted
    readonly property bool valueAvailable: kind === "brightness"
        ? brightnessAvailable : audioAvailable
    readonly property real level: Math.max(0, Math.min(1,
        kind === "brightness" ? brightnessLevel
            : (audioAvailable ? activeAudio.volume : 0)))
    readonly property string title: kind === "brightness" ? "Brightness"
        : kind === "microphone" ? "Microphone" : "Volume"
    readonly property string status: {
        if (kind === "brightness" && !brightnessKnown)
            return "…";
        if (!valueAvailable)
            return "Unavailable";
        if (muted)
            return "Muted";
        return Math.round(level * 100) + "%";
    }
    readonly property string iconSource: kind === "brightness" ? "icons/sun.svg"
        : kind === "microphone" ? "icons/microphone.svg"
        : muted ? "icons/speaker-slash.svg" : "icons/speaker-high.svg"

    function resolveScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    function reveal(nextKind, screenName) {
        kind = nextKind;
        targetScreenName = screenName;
        closeTimer.stop();
        if (!windowVisible) {
            windowVisible = true;
            presented = false;
            Qt.callLater(() => {
                if (osd.windowVisible)
                    osd.presented = true;
            });
        } else {
            presented = true;
        }
        hideTimer.restart();
    }

    function showOutput(screenName) {
        reveal("output", screenName);
    }

    function showMicrophone(screenName) {
        reveal("microphone", screenName);
    }

    function showBrightness(screenName) {
        reveal("brightness", screenName);
        if (brightnessQuery.running) {
            brightnessQueryPending = true;
        } else {
            brightnessQuery.running = true;
        }
    }

    function readBrightness(text) {
        brightnessKnown = true;
        brightnessAvailable = false;
        const rows = String(text || "").trim().split("\n");
        for (const row of rows) {
            if (row === "")
                continue;
            const fields = row.split(",");
            if (fields.length < 5 || fields[1] !== "backlight")
                continue;
            const parsed = parseFloat(String(fields[4]).replace("%", ""));
            if (!isNaN(parsed)) {
                brightnessLevel = Math.max(0, Math.min(1, parsed / 100));
                brightnessAvailable = true;
                return;
            }
        }
    }

    PwObjectTracker {
        objects: [osd.sink, osd.source].filter(node => node !== null)
    }

    Process {
        id: brightnessQuery
        command: ["brightnessctl", "--class=backlight", "--machine-readable"]
        stdout: StdioCollector {
            onStreamFinished: osd.readBrightness(text)
        }
        stderr: StdioCollector {}
        onRunningChanged: {
            if (!running && osd.brightnessQueryPending) {
                osd.brightnessQueryPending = false;
                Qt.callLater(() => brightnessQuery.running = true);
            }
        }
    }

    Timer {
        id: hideTimer
        interval: 1250
        onTriggered: {
            osd.presented = false;
            closeTimer.restart();
        }
    }

    Timer {
        id: closeTimer
        interval: Theme.reduceMotion ? 1 : 140
        onTriggered: osd.windowVisible = false
    }

    IpcHandler {
        target: "osd"

        function output(screenName: string): void {
            osd.showOutput(screenName);
        }

        function microphone(screenName: string): void {
            osd.showMicrophone(screenName);
        }

        function brightness(screenName: string): void {
            osd.showBrightness(screenName);
        }
    }

    PanelWindow {
        id: window
        visible: osd.windowVisible
        screen: osd.resolveScreen()
        implicitWidth: 326
        implicitHeight: 76
        color: "transparent"
        focusable: false
        aboveWindows: true
        exclusiveZone: 0
        surfaceFormat.opaque: false

        anchors.bottom: true
        margins.bottom: BarState.position === "bottom"
            ? BarState.thickness + 38 : 54

        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "garage-osd"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        mask: Region {}

        ContinuousRectangle {
            id: panel
            anchors.fill: parent
            opacity: osd.presented ? 1 : 0
            color: Theme.contentTint
            borderWidth: 1
            borderColor: Theme.frameOuter
            radius: 14
            power: Theme.cornerPower

            Behavior on opacity {
                NumberAnimation {
                    duration: Theme.reduceMotion ? 0 : 125
                    easing.type: osd.presented ? Easing.OutCubic : Easing.InCubic
                }
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 18
                anchors.rightMargin: 18
                spacing: 14

                Item {
                    Layout.preferredWidth: 24
                    Layout.preferredHeight: 24

                    Image {
                        id: icon
                        anchors.fill: parent
                        source: osd.iconSource
                        sourceSize.width: 48
                        sourceSize.height: 48
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        visible: false
                    }

                    ColorOverlay {
                        anchors.fill: icon
                        source: icon
                        color: osd.muted ? Theme.textDisabled : Theme.text
                        cached: true
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12

                        Text {
                            Layout.fillWidth: true
                            text: osd.title
                            color: Theme.text
                            font.family: Theme.sans
                            font.pixelSize: 13
                            font.weight: Font.DemiBold
                            renderType: Text.NativeRendering
                        }

                        Text {
                            text: osd.status
                            color: osd.muted || !osd.valueAvailable
                                ? Theme.textMuted : Theme.text
                            font.family: Theme.mono
                            font.pixelSize: 12
                            font.weight: Font.Medium
                            renderType: Text.NativeRendering
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 6
                        radius: height / 2
                        color: Theme.border

                        Rectangle {
                            width: parent.width * (osd.muted ? 0 : osd.level)
                            height: parent.height
                            radius: parent.radius
                            color: Theme.text

                            Behavior on width {
                                NumberAnimation {
                                    duration: Theme.reduceMotion ? 0 : 90
                                    easing.type: Easing.OutCubic
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
