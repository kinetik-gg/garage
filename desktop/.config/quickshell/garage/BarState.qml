pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

// The bar's rendered state, pushed from two watched markers.
//
// garage writes ~/.local/state/garage/generated/bar-layout.json on every [bar] and
// workspaces.indicator change, and clock-format.json on every region change -- both with
// write_marker, so this file's watches survive. There is no reload signal anywhere in
// the chain: the write is the apply, exactly as it is for Theme.
//
// Every value falls back to the shipped defaults when a marker has not landed yet --
// which is only possible before garage's first render -- so an absent file means the
// shipped bar, not a broken one.
QtObject {
    id: barState

    // -- Layout marker -------------------------------------------------------

    property int height: 43
    property real paddingScale: 1.2
    property string background: "transparent"
    property bool indicator: true
    property bool mediaPlayer: true
    property bool aiUsage: true
    property var monitors: ({ cpu: true, memory: true, network: false,
        temp: false, disk: false, gpu: false })

    // -- Clock marker --------------------------------------------------------

    property string clockLocale: ""
    property string dateFormat: "dmy"
    property string timeFormat: "24"
    property string firstDayOfWeek: "sunday"

    // Pushed from the signals rather than bound through text(), for the reason Theme
    // documents: text() assigns FileView properties as a side effect, so a binding built
    // on it does not reliably re-evaluate on change.
    property FileView layoutFile: FileView {
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/bar-layout.json"
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                const object = JSON.parse(String(text()));
                if (object === null || typeof object !== "object")
                    return;
                if (typeof object.height === "number")
                    height = object.height;
                if (typeof object.padding_scale === "number")
                    paddingScale = object.padding_scale;
                if (typeof object.background === "string")
                    background = object.background;
                if (typeof object.indicator === "boolean")
                    indicator = object.indicator;
                if (typeof object.media_player === "boolean")
                    mediaPlayer = object.media_player;
                if (typeof object.ai_usage === "boolean")
                    aiUsage = object.ai_usage;
                if (object.monitors !== null && typeof object.monitors === "object")
                    monitors = object.monitors;
            } catch (error) {
                // A truncated read is re-read on the next change; nothing to log.
            }
        }
    }

    property FileView clockFile: FileView {
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/clock-format.json"
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
        module: 12, image: 8, icon: 10, tray: 8
    })

    function roundHalfEven(value) {
        const floor = Math.floor(value);
        const diff = value - floor;
        if (diff !== 0.5)
            return Math.round(value);
        return floor % 2 === 0 ? floor : floor + 1;
    }

    function scaled(name) {
        return roundHalfEven(paddingTable[name] * paddingScale);
    }
}
