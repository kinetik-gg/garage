pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

// The bar's rendered state, pushed from two watched markers.
//
// Marker v2 carries composition as three ordered extension-id rails. It is data,
// not QML: the registry decides which ids exist, and an unknown id is skipped.
// garage writes both marker files with write_marker, so these watches survive.
//
// Every value falls back to the shipped defaults when a marker has not landed yet --
// which is only possible before garage's first render -- so an absent file means the
// shipped bar, not a broken one.
QtObject {
    id: barState

    // -- Layout marker -------------------------------------------------------

    property string position: "top"
    property int height: 43
    property real paddingScale: 1.2
    property string background: "transparent"
    property int maxGroupWidgets: 6
    property var left: ["menu", "workspaces"]
    property var center: ["media"]
    property var right: ["system", "tray", "notifications", "launcher",
        "control-center", "clock"]

    readonly property bool vertical: position === "left" || position === "right"
    readonly property bool horizontal: !vertical
    readonly property int thickness: height

    // -- Clock marker --------------------------------------------------------

    property string clockLocale: ""
    property string dateFormat: "dmy"
    property string timeFormat: "24"
    property string firstDayOfWeek: "sunday"

    // Pushed from the signals rather than bound through text(), for the reason Theme
    // documents: text() assigns FileView properties as a side effect, so a binding built
    // on it does not reliably re-evaluate on change.
    property FileView layoutFile: FileView {
        path: GaragePaths.barLayout
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                const object = JSON.parse(String(text()));
                if (object === null || typeof object !== "object")
                    return;
                if (BarState.validPosition(object.position))
                    position = object.position;
                if (typeof object.height === "number")
                    height = object.height;
                if (typeof object.padding_scale === "number")
                    paddingScale = object.padding_scale;
                if (typeof object.background === "string")
                    background = object.background;
                if (typeof object.max_group_widgets === "number")
                    maxGroupWidgets = object.max_group_widgets;
                if (Array.isArray(object.left))
                    left = BarState.extensionIds(object.left);
                if (Array.isArray(object.center))
                    center = BarState.extensionIds(object.center);
                if (Array.isArray(object.right))
                    right = BarState.extensionIds(object.right);
            } catch (error) {
                // A truncated read is re-read on the next change; nothing to log.
            }
        }
    }

    property FileView clockFile: FileView {
        path: GaragePaths.clockFormat
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                const object = JSON.parse(String(text()));
                if (object === null || typeof object !== "object")
                    return;
                if (typeof object.locale === "string")
                    clockLocale = object.locale;
                if (typeof object.date_format === "string")
                    dateFormat = object.date_format;
                if (typeof object.time_format === "string")
                    timeFormat = object.time_format;
                if (typeof object.first_day_of_week === "string")
                    firstDayOfWeek = object.first_day_of_week;
            } catch (error) {
                // Same contract as the layout marker above.
            }
        }
    }

    // -- Spacing -------------------------------------------------------------

    // PADDING_TABLE from the backend's spacing module, at scale 1.0. Scaled values use
    // round-half-to-even, which is what the old stylesheet generator emitted -- Python's
    // banker's rounding, not QML's Math.round -- so the paddings move to the same px at
    // every notch of the slider.
    readonly property var paddingTable: ({
        edge: 18, menuRight: 13, workspaceGap: 5,
        module: 12, image: 8, icon: 10, tray: 8, tooltip: 8
    })

    function roundHalfEven(value) {
        const floor = Math.floor(value);
        const diff = value - floor;
        if (diff !== 0.5)
            return Math.round(value);
        return floor % 2 === 0 ? floor : floor + 1;
    }

    function scaled(name) {
        const base = paddingTable[name];
        return typeof base === "number" ? roundHalfEven(base * paddingScale) : 0;
    }

    function validPosition(value) {
        return value === "top" || value === "bottom"
            || value === "left" || value === "right";
    }

    function extensionIds(value) {
        const ids = [];
        for (const candidate of value) {
            const id = String(candidate || "").trim();
            if (id !== "")
                ids.push(id);
        }
        return ids;
    }
}
