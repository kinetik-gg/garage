import QtQuick

// An HH:MM picker built from two combos.
//
// This replaced a free-text field. The backend rejects a malformed time, but
// PreferencesController fires the helper detached, so the error was discarded
// and the field simply snapped back a moment later with no explanation. Picking
// from a list makes an invalid value unrepresentable instead.
Row {
    id: field

    // Current value as "HH:MM". Values off the minute step are shown rounded but
    // are not written back until the user actually picks something.
    property string value: "00:00"
    property int minuteStep: 5

    signal committed(string value)

    spacing: 6

    readonly property var parts: String(field.value).split(":")
    readonly property int hour: {
        const parsed = parseInt(field.parts[0], 10);
        return isNaN(parsed) ? 0 : Math.max(0, Math.min(23, parsed));
    }
    readonly property int minute: {
        const parsed = parseInt(field.parts[1], 10);
        return isNaN(parsed) ? 0 : Math.max(0, Math.min(59, parsed));
    }

    readonly property var hourLabels: {
        const rows = [];
        for (let h = 0; h < 24; ++h)
            rows.push(String(h).padStart(2, "0"));
        return rows;
    }
    readonly property var minuteLabels: {
        const rows = [];
        for (let m = 0; m < 60; m += field.minuteStep)
            rows.push(String(m).padStart(2, "0"));
        return rows;
    }

    // Snap to the step here rather than at the call sites: changing the hour
    // carries the current minute along, and an off-step one would otherwise be
    // written back while the combo displays the rounded value.
    function commit(nextHour, nextMinute) {
        const step = Math.max(1, field.minuteStep);
        const snapped = Math.min(59, Math.round(nextMinute / step) * step);
        field.committed(String(nextHour).padStart(2, "0")
            + ":" + String(snapped).padStart(2, "0"));
    }

    SettingsCombo {
        implicitWidth: 68
        model: field.hourLabels
        currentIndex: field.hour
        onActivated: index => field.commit(index, field.minute)
    }

    Text {
        anchors.verticalCenter: parent.verticalCenter
        text: ":"
        color: Theme.textMuted
        font.family: Theme.sans
        font.pixelSize: 12
        renderType: Text.NativeRendering
    }

    SettingsCombo {
        implicitWidth: 68
        model: field.minuteLabels
        // Round to the nearest step so an off-step value from a hand-edited
        // preferences file still shows the closest entry rather than blank.
        currentIndex: Math.min(field.minuteLabels.length - 1,
            Math.round(field.minute / field.minuteStep))
        onActivated: index => field.commit(field.hour, index * field.minuteStep)
    }
}
