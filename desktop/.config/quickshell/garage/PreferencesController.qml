pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

// One preferences backend for the whole shell. The settings window and the
// control centre used to each instantiate a private copy, which meant two
// optimistic snapshots and two command queues that could disagree about what
// was set -- the queue below only serializes writes that actually share it.
Singleton {
    id: controller

    readonly property string helper: GaragePaths.garage
    readonly property string indexHelper: GaragePaths.fileIndex
    property var snapshot: ({
        preferences: { appearance: {}, input: {}, lock: {} },
        displays: [], audio: { outputs: [], inputs: [] },
        notifications: { available: false, dnd: false },
        inputCapabilities: { hasTouchpad: false }, capabilities: {}, error: ""
    })
    property bool loading: true
    // False only until the first snapshot lands. Every later refresh has data to
    // draw from, so the pane must not be papered over with a loading label while
    // one runs -- a snapshot shells out to hyprctl, pactl and timedatectl, and
    // that overlay was flashing over the pane on every wallpaper change.
    property bool ready: false
    property bool wallpaperPickerOpen: false
    property bool indexDirectoryPickerOpen: false
    property var indexStatus: ({
        enabled: false, activity: "loading", count: 0,
        last_scan_epoch: 0, last_scan_duration_ms: 0, error: ""
    })
    readonly property bool indexRefreshing: indexRefreshProcess.running
    // Which preference the shared picker writes, and the folder it opens in. The
    // wallpaper is per appearance now, so the picker cannot know its own target.
    property string wallpaperPickerKey: "wallpaper_dark"
    property string wallpaperPickerFolder: ""
    // Which half of the wallpaper schema the Wallpaper pane edits, and which of
    // its tabs is showing. Both live here rather than in the pane: the sidebar
    // drives Loader.setSource, which destroys and recreates the pane on every
    // visit. Seeded from the appearance on screen, which is the half meant
    // almost every time, and left wherever the user puts it after that.
    property string wallpaperScheme: Theme.scheme
    property int wallpaperTab: 0
    property string error: ""
    // Backend commands run one at a time: the helper's "set" is an unlocked
    // read-modify-write, so overlapping processes clobber each other's keys.
    property var pending: []
    property string commandError: ""
    property bool pendingRefresh: false
    property string displayToken: ""
    property int displaySeconds: 0
    property bool displayPending: displayToken !== ""
    property var displayPreviousSnapshot: []

    signal changed()

    function refresh() {
        // Snapshots shell out to hyprctl/pactl/timedatectl and can outlast the
        // refresh timer, so re-arm instead of dropping the request on the floor.
        if (snapshotProcess.running) {
            pendingRefresh = true;
            return;
        }
        pendingRefresh = false;
        loading = true;
        snapshotProcess.running = true;
    }

    function enqueue(command) {
        pending.push(command);
        runNextCommand();
    }

    function runNextCommand() {
        if (commandProcess.running || pending.length === 0)
            return;
        commandProcess.command = pending[0];
        commandProcess.running = true;
    }

    // A rejected command must outlive the refresh it schedules, otherwise the
    // snapshot handler wipes the banner before it can be read. Success only
    // clears the banner when it is still showing this layer's message.
    function reportCommand(message) {
        if (message !== "" || error === commandError)
            error = message;
        commandError = message;
    }

    function preference(section, key, fallback) {
        const group = snapshot.preferences && snapshot.preferences[section];
        return group && group[key] !== undefined ? group[key] : fallback;
    }

    function setPreference(section, key, value) {
        const next = JSON.parse(JSON.stringify(snapshot));
        if (!next.preferences[section])
            next.preferences[section] = {};
        next.preferences[section][key] = value;
        snapshot = next;
        if (section === "appearance" && key === "accent_color")
            Theme.accentName = String(value);
        changed();
        enqueue([helper, "set", section + "." + key, JSON.stringify(value)]);
        // The optimistic value above is already exactly what the pane draws, and
        // the preferences.toml watch below still refreshes for correctness. A
        // second, immediate refresh only reassigns the whole snapshot object --
        // re-evaluating every binding in the pane and re-decoding the wallpaper
        // preview -- which is what made a colour or picture change visibly reset
        // the page it was made on.
        const isLocal = section === "appearance"
            && (key === "accent_color" || key.startsWith("wallpaper"));
        if (!isLocal)
            refreshTimer.restart();
    }

    function addIndexDirectory(path) {
        const value = String(path || "").trim();
        if (value === "")
            return;
        const current = String(controller.preference(
            "indexing", "directories", "")).split("\n")
            .map(item => item.trim()).filter(Boolean);
        if (current.indexOf(value) !== -1)
            return;
        controller.setPreference("indexing", "directories",
            current.concat([value]).join("\n"));
    }

    // -- Bar composition -----------------------------------------------------
    // Each bar rail is one \n-joined scalar, edited read-modify-write here.
    // Every rewrite goes through setPreference, so it rides the serialized
    // command queue above -- the Menu Bar pane and the Workspaces pane cannot
    // interleave the halves of a move into each other's writes.

    // Mirrors of the schema defaults, so a toggle before the first snapshot
    // lands edits the rails the bar is actually drawing, not an empty list.
    readonly property var barRailDefaults: ({
        left: "menu\nworkspaces",
        center: "media",
        right: "system\ntray\nnotifications\nlauncher\ncontrol-center\nclock"
    })
    readonly property var barGroups: ["left", "center", "right"]

    function barList(group) {
        if (barGroups.indexOf(group) === -1)
            return [];
        const fallback = barRailDefaults[group] !== undefined
            ? barRailDefaults[group] : "";
        return String(controller.preference("bar", "widgets_" + group, fallback))
            .split("\n").map(item => item.trim()).filter(Boolean);
    }

    function barListToggle(id, group) {
        if (barGroups.indexOf(group) === -1)
            return;
        const list = controller.barList(group);
        const at = list.indexOf(id);
        if (at === -1)
            list.push(id);
        else
            list.splice(at, 1);
        controller.setPreference("bar", "widgets_" + group, list.join("\n"));
    }

    function barListMove(id, group, delta) {
        if (barGroups.indexOf(group) === -1)
            return;
        const list = controller.barList(group);
        const at = list.indexOf(id);
        const destination = at + delta;
        if (at === -1 || destination < 0 || destination >= list.length)
            return;
        list.splice(at, 1);
        list.splice(destination, 0, id);
        controller.setPreference("bar", "widgets_" + group, list.join("\n"));
    }

    // An empty destination means unanchored. Inspect every live rail rather
    // than trusting the row's source label: this also repairs an id inherited
    // in two rails, removing every duplicate while preserving its position if
    // it is already in the requested destination.
    function barListSetGroup(id, from, to) {
        if (from === to)
            return;
        if (to !== "" && barGroups.indexOf(to) === -1)
            return;
        for (const group of barGroups) {
            const current = controller.barList(group);
            const at = current.indexOf(id);
            const next = current.filter(candidate => candidate !== id);
            if (group === to) {
                const destination = at === -1 ? next.length : Math.min(at, next.length);
                next.splice(destination, 0, id);
            }
            if (next.join("\n") !== current.join("\n"))
                controller.setPreference("bar", "widgets_" + group, next.join("\n"));
        }
    }

    function refreshIndexStatus() {
        if (!indexStatusProcess.running)
            indexStatusProcess.running = true;
    }

    function refreshIndex() {
        if (indexRefreshProcess.running
                || !controller.preference("indexing", "enabled", true))
            return;
        const next = Object.assign({}, indexStatus);
        next.activity = "indexing";
        next.error = "";
        indexStatus = next;
        indexRefreshProcess.running = true;
    }

    function action(name, value) {
        const command = [helper, "action", name];
        if (value !== undefined)
            command.push(JSON.stringify(value));
        enqueue(command);
        refreshTimer.restart();
    }

    function testDisplays(layout) {
        if (displayProcess.running)
            return;
        error = "";
        commandError = "";
        displayPreviousSnapshot = JSON.parse(JSON.stringify(snapshot.displays || []));
        const optimistic = JSON.parse(JSON.stringify(snapshot));
        optimistic.displays = JSON.parse(JSON.stringify(layout.displays || []));
        snapshot = optimistic;
        changed();
        displayProcess.command = [helper, "display-test", JSON.stringify(layout)];
        displayProcess.running = true;
    }

    function confirmDisplays() {
        if (!displayToken)
            return;
        Quickshell.execDetached([helper, "display-confirm", displayToken]);
        displayToken = "";
        displaySeconds = 0;
        displayPreviousSnapshot = [];
        refreshTimer.restart();
    }

    function revertDisplays() {
        if (!displayToken)
            return;
        Quickshell.execDetached([helper, "display-revert", displayToken]);
        if (displayPreviousSnapshot.length > 0) {
            const restored = JSON.parse(JSON.stringify(snapshot));
            restored.displays = JSON.parse(JSON.stringify(displayPreviousSnapshot));
            snapshot = restored;
            changed();
        }
        displayToken = "";
        displaySeconds = 0;
        displayPreviousSnapshot = [];
        refreshTimer.restart();
    }

    Component.onCompleted: {
        refresh();
        refreshIndexStatus();
    }

    Process {
        id: snapshotProcess
        command: [controller.helper, "snapshot"]
        onRunningChanged: if (!running && controller.pendingRefresh) controller.refresh()
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const response = JSON.parse(text);
                    if (!response.ok)
                        throw new Error(response.error || "Unable to read settings");
                    if (controller.displayPreviousSnapshot.length > 0)
                        response.data.displays = controller.snapshot.displays;
                    controller.snapshot = response.data;
                    const appearance = response.data.preferences.appearance || {};
                    Theme.accentName = String(appearance.accent_color || "blue");
                    controller.error = response.data.error || controller.commandError;
                    controller.changed();
                } catch (failure) {
                    controller.error = String(failure);
                }
                controller.loading = false;
                controller.ready = true;
            }
        }
    }

    Process {
        id: commandProcess
        // streamFinished lands before running clears, so the queue only advances
        // once the process object is genuinely free to be restarted.
        onRunningChanged: {
            if (running)
                return;
            controller.pending.shift();
            controller.runNextCommand();
        }
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const response = JSON.parse(text);
                    controller.reportCommand(response.ok ? "" : (response.error || "Command failed"));
                } catch (failure) {
                    controller.reportCommand("No response from garage");
                }
            }
        }
    }

    Process {
        id: indexStatusProcess
        command: [controller.indexHelper, "status"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const response = JSON.parse(text);
                    if (!response.ok)
                        throw new Error(response.error || "Unable to read index status");
                    controller.indexStatus = response.data;
                } catch (failure) {
                    const next = Object.assign({}, controller.indexStatus);
                    next.activity = "error";
                    next.error = String(failure);
                    controller.indexStatus = next;
                }
            }
        }
    }

    Process {
        id: indexRefreshProcess
        command: [controller.indexHelper, "refresh"]
        onRunningChanged: if (!running) controller.refreshIndexStatus()
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const response = JSON.parse(text);
                    if (!response.ok)
                        throw new Error(response.error || "Unable to refresh the index");
                } catch (failure) {
                    const next = Object.assign({}, controller.indexStatus);
                    next.activity = "error";
                    next.error = String(failure);
                    controller.indexStatus = next;
                }
            }
        }
    }

    Process {
        id: displayProcess
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const response = JSON.parse(text);
                    if (!response.ok)
                        throw new Error(response.error || "Unable to test display layout");
                    controller.displayToken = response.data.token;
                    controller.displaySeconds = 15;
                    displayCountdown.restart();
                } catch (failure) {
                    controller.error = String(failure);
                    if (controller.displayPreviousSnapshot.length > 0) {
                        const restored = JSON.parse(JSON.stringify(controller.snapshot));
                        restored.displays = JSON.parse(JSON.stringify(controller.displayPreviousSnapshot));
                        controller.snapshot = restored;
                        controller.displayPreviousSnapshot = [];
                        controller.changed();
                    }
                }
            }
        }
    }

    Timer {
        id: refreshTimer
        interval: 650
        onTriggered: controller.refresh()
    }

    Timer {
        interval: 2500
        repeat: true
        running: true
        onTriggered: controller.refreshIndexStatus()
    }

    Timer {
        id: displayCountdown
        interval: 1000
        repeat: true
        onTriggered: {
            controller.displaySeconds--;
            if (controller.displaySeconds <= 0) {
                stop();
                controller.revertDisplays();
            }
        }
    }

    FileView {
        path: Quickshell.env("HOME") + "/.config/garage/preferences.toml"
        printErrors: false
        watchChanges: true
        onFileChanged: refreshTimer.restart()
    }

    FileView {
        path: Quickshell.env("HOME") + "/.config/garage/displays.toml"
        printErrors: false
        watchChanges: true
        onFileChanged: refreshTimer.restart()
    }
}
