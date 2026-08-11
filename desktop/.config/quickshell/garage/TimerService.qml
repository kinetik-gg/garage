pragma Singleton
pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Io
import QtQuick
import "LauncherExtras.js" as LauncherExtras

Singleton {
    id: service

    signal changed()

    property double nowMs: Date.now()
    readonly property int maxTimers: 16
    property var timers: []
    property bool stopwatchRunning: false
    property double stopwatchStartedAt: 0
    property double stopwatchBaseElapsedMs: 0
    property var stopwatchLaps: []
    readonly property double stopwatchElapsedMs: Math.max(0,
        service.stopwatchBaseElapsedMs
        + (service.stopwatchRunning
            ? service.nowMs - service.stopwatchStartedAt : 0))

    function persist() {
        stateFile.setText(JSON.stringify({
            timers: service.timers,
            stopwatchRunning: service.stopwatchRunning,
            stopwatchStartedAt: service.stopwatchStartedAt,
            stopwatchElapsedMs: service.stopwatchBaseElapsedMs,
            stopwatchLaps: service.stopwatchLaps
        }));
        service.changed();
    }

    function loadState() {
        try {
            const value = JSON.parse(String(stateFile.text() || "{}"));
            service.timers = Array.isArray(value.timers) ? value.timers : [];
            service.stopwatchRunning = value.stopwatchRunning === true;
            service.stopwatchStartedAt = Number(value.stopwatchStartedAt || 0);
            service.stopwatchBaseElapsedMs = Math.max(0,
                Number(value.stopwatchElapsedMs || 0));
            service.stopwatchLaps = Array.isArray(value.stopwatchLaps)
                ? value.stopwatchLaps.map(Number).filter(isFinite).slice(-50) : [];
        } catch (error) {
            service.timers = [];
            service.stopwatchRunning = false;
            service.stopwatchStartedAt = 0;
            service.stopwatchBaseElapsedMs = 0;
            service.stopwatchLaps = [];
        }
    }

    function startTimer(durationMs, label) {
        if (service.timers.length >= service.maxTimers)
            return false;
        const started = Date.now();
        service.timers = service.timers.concat([{
            id: String(started) + "-" + String(service.timers.length),
            label: String(label || "Timer"),
            startedAt: started,
            deadline: started + Number(durationMs)
        }]);
        service.nowMs = started;
        service.persist();
        return true;
    }

    function cancelTimer(identifier) {
        const next = service.timers.filter(timer => String(timer.id) !== String(identifier));
        if (next.length === service.timers.length)
            return;
        service.timers = next;
        service.persist();
    }

    function startStopwatch() {
        if (service.stopwatchRunning)
            return;
        service.stopwatchStartedAt = Date.now();
        service.stopwatchRunning = true;
        service.persist();
    }

    function pauseStopwatch() {
        if (!service.stopwatchRunning)
            return;
        service.stopwatchBaseElapsedMs = service.stopwatchElapsedMs;
        service.stopwatchStartedAt = 0;
        service.stopwatchRunning = false;
        service.persist();
    }

    function resetStopwatch() {
        service.stopwatchBaseElapsedMs = 0;
        service.stopwatchStartedAt = 0;
        service.stopwatchRunning = false;
        service.stopwatchLaps = [];
        service.nowMs = Date.now();
        service.persist();
    }

    function lapStopwatch() {
        if (!service.stopwatchRunning)
            return;
        service.stopwatchLaps = service.stopwatchLaps.concat([
            service.stopwatchElapsedMs
        ]).slice(-50);
        service.persist();
    }

    function rowsFor(input) {
        const timer = LauncherExtras.timerSpec(input);
        if (timer !== null) {
            if (timer.mode === "error")
                return [{ kind: "error", title: timer.title, subtitle: timer.subtitle }];
            if (timer.mode === "start")
                return [{ kind: "clock-timer-start",
                    title: "Start " + LauncherExtras.compactDuration(timer.durationMs) + " timer",
                    subtitle: timer.label, action: "timer-start",
                    durationMs: timer.durationMs, label: timer.label }];
            if (service.timers.length === 0)
                return [{ kind: "status", title: "No active timers",
                    subtitle: "Try timer 10m Tea" }];
            return service.timers.map(active => ({
                kind: timer.mode === "cancel" ? "clock-timer-cancel" : "status",
                title: (timer.mode === "cancel" ? "Cancel " : "") + String(active.label),
                subtitle: LauncherExtras.clockDuration(
                    Number(active.deadline) - service.nowMs, false) + " remaining"
                    + (timer.mode === "cancel" ? "" : " — use timer cancel to remove"),
                action: timer.mode === "cancel" ? "timer-cancel" : "",
                timerId: String(active.id)
            }));
        }

        const stopwatch = LauncherExtras.stopwatchSpec(input);
        if (stopwatch === null)
            return null;
        if (stopwatch.action === "error")
            return [{ kind: "error", title: stopwatch.title, subtitle: stopwatch.subtitle }];
        const rows = [{ kind: "status",
            title: "Stopwatch  " + LauncherExtras.clockDuration(
                service.stopwatchElapsedMs, true),
            subtitle: service.stopwatchRunning ? "Running" : "Paused" }];
        const requested = stopwatch.action;
        function actionRow(action, title, subtitle) {
            return { kind: "clock-stopwatch-" + action, title: title,
                subtitle: subtitle, action: "stopwatch-" + action };
        }
        if (requested !== "list") {
            let control = null;
            if (requested === "start" || requested === "resume") {
                if (!service.stopwatchRunning)
                    control = actionRow("start", "Start Stopwatch", "Stopwatch control");
            } else if (requested === "pause") {
                if (service.stopwatchRunning)
                    control = actionRow("pause", "Pause Stopwatch", "Stopwatch control");
            } else if (requested === "lap") {
                if (service.stopwatchRunning)
                    control = actionRow("lap", "Record Lap", "Stopwatch control");
            } else if (requested === "reset") {
                control = actionRow("reset", "Reset Stopwatch", "Stopwatch control");
            }
            // Exact control queries need their action selected by default. The
            // live status remains useful, but comes second so Return performs
            // what the user typed instead of selecting a non-actionable row.
            return control === null ? rows : [control].concat(rows);
        }
        rows.push(service.stopwatchRunning
            ? actionRow("pause", "Pause Stopwatch", "Stopwatch control")
            : actionRow("start", "Start Stopwatch", "Stopwatch control"));
        if (service.stopwatchRunning)
            rows.push(actionRow("lap", "Record Lap", "Stopwatch control"));
        if (service.stopwatchElapsedMs > 0 || service.stopwatchLaps.length > 0)
            rows.push(actionRow("reset", "Reset Stopwatch", "Clear elapsed time and laps"));
        for (let index = service.stopwatchLaps.length - 1; index >= 0; --index)
            rows.push({ kind: "status", title: "Lap " + String(index + 1),
                subtitle: LauncherExtras.clockDuration(service.stopwatchLaps[index], true) });
        return rows;
    }

    function activate(row) {
        const action = String(row.action || "");
        if (action === "timer-start")
            service.startTimer(row.durationMs, row.label);
        else if (action === "timer-cancel")
            service.cancelTimer(row.timerId);
        else if (action === "stopwatch-start")
            service.startStopwatch();
        else if (action === "stopwatch-pause")
            service.pauseStopwatch();
        else if (action === "stopwatch-lap")
            service.lapStopwatch();
        else if (action === "stopwatch-reset")
            service.resetStopwatch();
    }

    function tick() {
        service.nowMs = Date.now();
        const expired = service.timers.filter(timer => Number(timer.deadline) <= service.nowMs);
        if (expired.length === 0) {
            service.changed();
            return;
        }
        const expiredIds = expired.map(timer => String(timer.id));
        service.timers = service.timers.filter(
            timer => expiredIds.indexOf(String(timer.id)) === -1);
        service.persist();
        for (const timer of expired)
            Quickshell.execDetached(["notify-send", "--app-name=Garage",
                "--urgency=normal", "Timer finished", String(timer.label)]);
    }

    Timer {
        interval: 250
        repeat: true
        running: service.timers.length > 0 || service.stopwatchRunning
        onTriggered: service.tick()
    }

    FileView {
        id: stateFile
        path: Quickshell.env("HOME") + "/.local/state/garage/clock-state.json"
        printErrors: false
        blockLoading: true
        atomicWrites: true
    }

    Component.onCompleted: {
        service.loadState();
        service.tick();
    }
}
