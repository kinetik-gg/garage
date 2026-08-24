pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick
import QtQml.Models

// Owns optional manifest probes at shell scope: exactly one process per
// extension id, shared by every output's BarModule. Each stdout line is JSON.
Singleton {
    id: host

    property var probes: ({})
    property int revision: 0
    readonly property var services: ({
        barState: BarState,
        context: BarContext,
        metrics: MetricsService,
        media: MediaController,
        workspaces: WorkspaceService,
        theme: Theme,
        paths: GaragePaths
    })

    function lookup(id) {
        const ignored = revision;
        return probes[String(id)] || null;
    }

    function publish(id, probe) {
        const next = Object.assign({}, probes);
        if (probe)
            next[id] = probe;
        else
            delete next[id];
        probes = next;
        ++revision;
    }

    component ProbeSlot: QtObject {
        required property var entry

        property QtObject state: QtObject {
            property var data: ({})
            property bool connected: false
        }

        property Process process: Process {
            running: true
            command: entry.probe.command
            stdout: SplitParser {
                splitMarker: "\n"
                onRead: line => {
                    try {
                        state.data = JSON.parse(String(line));
                        state.connected = true;
                    } catch (error) {
                        // A partial or diagnostic line is not probe state.
                    }
                }
            }
            onExited: exitCode => {
                state.connected = false;
                restart.restart();
            }
        }

        property Timer restart: Timer {
            interval: entry.probe.restartMs || 2000
            onTriggered: process.running = true
        }

        Component.onCompleted: host.publish(entry.id, state)
        Component.onDestruction: host.publish(entry.id, null)
    }

    Instantiator {
        model: Object.values(ExtensionRegistry.entries)
            .filter(entry => entry.hasProbe)
        delegate: ProbeSlot { entry: modelData }
    }
}
