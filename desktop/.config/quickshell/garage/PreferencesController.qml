import Quickshell
import Quickshell.Io
import QtQuick

Scope {
    id: controller

    readonly property string helper: Quickshell.env("HOME") + "/.local/bin/garage"
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

    Component.onCompleted: refresh()

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
