pragma Singleton

import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import QtQuick

// One Hyprland workspace snapshot for every bar instance. A screen-local
// BarWorkspaces used to run its own pair of hyprctl processes for each output;
// this service reads the global state once, then projects it by connector name.
Singleton {
    id: workspaces

    property var entries: []
    property var monitors: []
    property int revision: 0

    function entriesFor(screenName) {
        // Reading revision makes calls from bindings re-evaluate after either
        // half of the snapshot changes, even when the filtered result is empty.
        const currentRevision = revision;
        return entries.filter(candidate => candidate.monitor === screenName)
            .sort((left, right) => left.id - right.id);
    }

    function activeFor(screenName) {
        const currentRevision = revision;
        const monitor = monitors.find(candidate => candidate.name === screenName);
        return monitor && monitor.activeWorkspace
            ? Number(monitor.activeWorkspace.id) : -1;
    }

    function refresh() {
        if (!workspaceProcess.running)
            workspaceProcess.running = true;
        if (!monitorProcess.running)
            monitorProcess.running = true;
    }

    function activate(id) {
        if (switchProcess.running)
            return;
        switchProcess.command = ["hyprctl", "dispatch",
            "hl.dsp.focus({ workspace = " + id + " })"];
        switchProcess.running = true;
    }

    Component.onCompleted: refresh()

    Connections {
        target: Hyprland

        function onRawEvent(event) {
            const name = String(event.name || "");
            if (name.startsWith("workspace") || name === "focusedmon"
                || name === "openwindow" || name === "closewindow"
                || name === "movewindow" || name === "urgency")
                debounce.restart();
        }
    }

    Timer {
        id: debounce

        interval: 150
        onTriggered: workspaces.refresh()
    }

    Process {
        id: workspaceProcess

        command: ["hyprctl", "-j", "workspaces"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const parsed = JSON.parse(text);
                    if (!Array.isArray(parsed))
                        return;
                    workspaces.entries = parsed;
                    workspaces.revision += 1;
                } catch (parseError) {
                    // A compositor mid-reload owes nobody a parse.
                }
            }
        }
    }

    Process {
        id: monitorProcess

        command: ["hyprctl", "-j", "monitors"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const parsed = JSON.parse(text);
                    if (!Array.isArray(parsed))
                        return;
                    workspaces.monitors = parsed;
                    workspaces.revision += 1;
                } catch (parseError) {
                    // Same contract as the workspace half above.
                }
            }
        }
    }

    Process {
        id: switchProcess

        command: ["hyprctl", "dispatch", ""]
    }
}
