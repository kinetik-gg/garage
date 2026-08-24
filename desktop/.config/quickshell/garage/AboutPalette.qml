import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

FloatingWindow {
    id: about

    required property string targetScreenName
    property real horizontalPadding: 40
    property real verticalPadding: 26
    property var facts: ({})
    property bool factsReady: false
    property string memoryType: ""
    property bool memoryProbeReady: false
    property var gpuMemory: ({})
    property bool gpuProbeReady: false

    signal dismissed()
    signal moreInfoRequested()

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === about.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    function module(data, type) {
        for (const item of data) {
            if (item.type === type)
                return item.result;
        }
        return null;
    }

    function formatBytes(bytes) {
        if (!bytes)
            return "Unknown";
        const tebibyte = 1099511627776;
        if (bytes >= tebibyte)
            return (bytes / tebibyte).toFixed(1).replace(".0", "") + " TB";
        return (bytes / 1073741824).toFixed(1).replace(".0", "") + " GB";
    }

    function loadFacts(text) {
        try {
            const data = JSON.parse(text);
            const os = module(data, "OS") || {};
            const kernel = module(data, "Kernel") || {};
            const cpu = module(data, "CPU") || {};
            const memory = module(data, "Memory") || {};
            const gpu = module(data, "GPU") || [];
            const swap = module(data, "Swap") || [];
            const disks = module(data, "Disk") || [];
            const rootDisk = disks.find(item => item.mountpoint === "/") || disks[0] || {};

            about.facts = {
                os: os.prettyName || os.name || "Arch Linux",
                kernel: kernel.release || "Unknown",
                cpu: cpu.cpu || "Unknown",
                cores: cpu.cores ? cpu.cores.physical + " cores / "
                    + cpu.cores.logical + " threads" : "Unknown",
                memory: formatBytes(memory.total),
                graphics: gpu.map(item => ({
                    name: item.name || "Unknown GPU",
                    memory: item.memory && item.memory.dedicated
                        ? item.memory.dedicated.total : 0
                })),
                swap: swap.length > 0
                    ? formatBytes(swap.reduce((total, item) => total + (item.total || 0), 0))
                    : "None",
                disk: rootDisk.bytes
                    ? formatBytes(rootDisk.bytes.total) + " — "
                        + (rootDisk.filesystem || "Unknown filesystem")
                    : "Unknown"
            };
            about.factsReady = true;
        } catch (error) {
            console.warn("Unable to read system information:", error);
            about.factsReady = true;
        }
    }

    function loadMemoryType(text) {
        const match = text.match(/spd[^\n]*\b(DDR[0-9]+)\b/i);
        about.memoryType = match ? match[1].toUpperCase() : "";
        about.memoryProbeReady = true;
    }

    function loadGpuMemory(text) {
        const detected = {};
        for (const line of text.trim().split("\n")) {
            const fields = line.split("\t");
            if (fields.length === 2 && Number(fields[1]) > 0)
                detected[fields[0]] = Number(fields[1]);
        }
        about.gpuMemory = detected;
        about.gpuProbeReady = true;
    }

    function gpuText(gpu) {
        let bytes = gpu.memory || 0;
        if (!bytes) {
            for (const heading of Object.keys(about.gpuMemory)) {
                const normalizedHeading = heading.toLowerCase();
                const normalizedName = gpu.name.toLowerCase();
                if (normalizedHeading.includes(normalizedName)
                        || normalizedName.split(" ").every(word => normalizedHeading.includes(word))) {
                    bytes = about.gpuMemory[heading];
                    break;
                }
            }
        }
        return gpu.name + (bytes ? " · " + formatBytes(bytes) + " VRAM" : "");
    }

    title: "About This System"
    implicitWidth: aboutContent.width + horizontalPadding * 2
    implicitHeight: aboutContent.implicitHeight + verticalPadding * 2
    color: "transparent"
    surfaceFormat.opaque: false
    visible: factsReady && memoryProbeReady && gpuProbeReady
        && logo.status === Image.Ready


    Rectangle {
        anchors.fill: parent
        color: Theme.contentTint

        ContinuousRectangle {
            id: closeButton
            anchors {
                top: parent.top
                right: parent.right
                topMargin: 14
                rightMargin: 14
            }
            width: closeIcon.width + 20
            height: width
            radius: Theme.controlRadius
            color: closePointer.containsMouse ? Theme.hoverStrong : "transparent"
            z: 1

            Image {
                id: closeIcon
                anchors.centerIn: parent
                width: 18
                height: 18
                source: "icons/x.svg"
                sourceSize.width: 36
                sourceSize.height: 36
                fillMode: Image.PreserveAspectFit
                smooth: true
                antialiasing: true
                mipmap: true
                visible: false
            }

            // The svg carries its own colour, so recolouring needs an overlay.
            ColorOverlay {
                anchors.fill: closeIcon
                source: closeIcon
                color: Theme.text
                cached: true
                z: 1
            }

            MouseArea {
                id: closePointer
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: about.dismissed()
            }
        }

        ColumnLayout {
            id: aboutContent
            anchors.centerIn: parent
            width: 320
            spacing: 0

            Item {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 14
                Layout.preferredWidth: 92
                Layout.preferredHeight: 92

                Image {
                    id: logo
                    anchors.fill: parent
                    source: "icons/archlinux-logo.svg"
                    sourceSize.width: 184
                    sourceSize.height: 184
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    antialiasing: true
                    mipmap: true
                }
            }

            Text {
                Layout.fillWidth: true
                Layout.topMargin: 18
                text: about.facts.kernel && about.facts.os
                    ? about.facts.kernel + " · " + about.facts.os
                    : "Detecting system…"
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 12
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }

            Item { Layout.preferredHeight: 22 }
            MenuSeparator { Layout.fillWidth: true }
            Item { Layout.preferredHeight: 18 }

            ColumnLayout {
                Layout.alignment: Qt.AlignHCenter
                spacing: 10

                AboutInfoRow { label: "Processor"; value: about.facts.cpu || "Detecting…" }
                AboutInfoRow { label: ""; value: about.facts.cores || "Detecting…" }
                AboutInfoRow { label: "Memory"; value: about.facts.memory || "Detecting…" }
                AboutInfoRow {
                    visible: about.memoryType !== ""
                    label: ""
                    value: about.memoryType
                }

                Repeater {
                    model: about.facts.graphics || []
                    AboutInfoRow {
                        required property int index
                        required property var modelData
                        label: index === 0 ? "GPU" : ""
                        value: about.gpuText(modelData)
                    }
                }

                AboutInfoRow { label: "Swap"; value: about.facts.swap || "Detecting…" }
                AboutInfoRow { label: "Disk"; value: about.facts.disk || "Detecting…" }
            }

            Item { Layout.preferredHeight: 22 }

            ContinuousRectangle {
                Layout.alignment: Qt.AlignHCenter
                implicitWidth: moreInfoText.implicitWidth + 28
                implicitHeight: moreInfoText.implicitHeight + 14
                radius: Theme.controlRadius
                color: moreInfoPointer.containsMouse ? Theme.accentHover : Theme.accent

                Text {
                    id: moreInfoText
                    anchors.centerIn: parent
                    text: "More Info…"
                    color: Theme.accentText
                    font.family: Theme.sans
                    font.pixelSize: 12
                    renderType: Text.NativeRendering
                }

                MouseArea {
                    id: moreInfoPointer
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        about.dismissed();
                        about.moreInfoRequested();
                    }
                }
            }

            Text {
                Layout.fillWidth: true
                Layout.topMargin: 20
                text: "The Arch Linux™ name and logo are recognized trademarks. "
                    + "Linux® is the registered trademark of Linus Torvalds. "
                    + "GNU software is developed by the Free Software Foundation and contributors."
                color: Theme.textDisabled
                font.family: Theme.sans
                font.pixelSize: 9
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                renderType: Text.NativeRendering
            }

        }
    }

    Process {
        command: ["fastfetch", "--format", "json"]
        running: true
        stdout: StdioCollector { onStreamFinished: about.loadFacts(text) }
    }

    Process {
        command: ["journalctl", "-k", "-b", "--no-pager"]
        running: true
        stdout: StdioCollector { onStreamFinished: about.loadMemoryType(text) }
    }

    Process {
        command: [GaragePaths.vramInfo]
        running: true
        stdout: StdioCollector { onStreamFinished: about.loadGpuMemory(text) }
    }

}
