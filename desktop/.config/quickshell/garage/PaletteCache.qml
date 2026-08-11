pragma Singleton

import QtQuick

// What a palette knew the last time it was open.
//
// The shell keeps its palettes in LazyLoaders, so a dismissal destroys the
// window and everything it had collected with it. Both the activity monitor and
// the AI usage panel start their collector as they map, which meant every
// reopen spent its first second showing "connecting…" over an empty panel for
// figures that had been on screen moments earlier -- and toggling a panel to
// glance at it twice is exactly what these two are for. This object outlives
// them: a palette hands over its reading as it arrives and seeds itself from
// the stored one on construction, so a reopen draws the previous numbers at
// once and replaces them when the collector answers.
//
// In memory, for the lifetime of the shell process, deliberately. Nothing here
// is written to disk: the point is a warm reopen, not a warm boot, and a
// snapshot restored from a previous session would be a wall of confident
// figures about a machine state that no longer exists.
//
// Each entry is whatever plain-JS shape its palette chose to store, opaque here
// -- this object has no business knowing what a VRAM history is. The timestamp
// beside it is the one thing it does own, because a seeded palette has to be
// able to say how old what it is showing is instead of presenting it as live.
QtObject {
    id: cache

    // MonitorPalette's last snapshot: the parsed stream object plus the series
    // the graphs are drawn from.
    property var monitorState: null
    property real monitorSavedAt: 0

    function saveMonitor(state) {
        cache.monitorState = state;
        cache.monitorSavedAt = Date.now();
    }

    // AiUsagePalette's last usable payload. Only the ones worth restoring are
    // stored -- an "unavailable" answer is knowledge about a tokscale install
    // rather than about usage, and replaying it would have the panel report a
    // missing CLI before it had looked for one this time.
    property var aiUsageState: null
    property real aiUsageSavedAt: 0

    function saveAiUsage(state) {
        cache.aiUsageState = state;
        cache.aiUsageSavedAt = Date.now();
    }

    // A stored entry's age in words, for the palette that is showing it.
    //
    // Takes the clock as an argument rather than reading it, so the label can be
    // a binding that ticks rather than a string frozen at construction: a panel
    // whose collector never answers would otherwise go on claiming its cached
    // reading was seconds old for as long as it stayed open.
    function formatAge(savedAt, now) {
        if (savedAt <= 0)
            return "";
        const seconds = Math.max(0, Math.round((now - savedAt) / 1000));
        if (seconds < 60)
            return seconds + "s ago";
        const minutes = Math.round(seconds / 60);
        if (minutes < 60)
            return minutes + "m ago";
        return Math.round(minutes / 60) + "h ago";
    }
}
