import QtQuick
import QtQuick.Controls as Controls

// The bar's clock: date and time from the region marker, and a month-grid popup
// on hover where waybar's calendar tooltip used to be.
//
// The format halves are the schema's enums mapped onto Qt patterns -- no strftime
// parsing anywhere. The calendar is hand-built rather than Controls.MonthGrid,
// which on this Qt carries no first-weekday, locale or week-number support at
// all; the contract is small: weeks start on the configured day, ISO week numbers
// appear down the left while weeks start on Monday, and today is accented.
Item {
    id: clock

    readonly property var locale: BarState.clockLocale !== ""
        ? Qt.locale(BarState.clockLocale) : Qt.locale()

    // %a %d %b / %a %b %d / %a %Y-%m-%d against %H:%M or %I:%M %p, joined by the
    // two spaces the old template carried between the halves.
    readonly property string pattern: {
        const date = BarState.dateFormat === "iso" ? "ddd yyyy-MM-dd"
            : BarState.dateFormat === "mdy" ? "ddd MMM dd" : "ddd dd MMM";
        const time = BarState.timeFormat === "12" ? "hh:mm AP" : "HH:mm";
        return date + "  " + time;
    }

    property date now: new Date()
    readonly property bool mondayFirst: BarState.firstDayOfWeek === "monday"

    implicitWidth: clockText.implicitWidth + BarState.scaled("module") * 2
    implicitHeight: Math.max(clockText.implicitHeight + 8, 24)

    Timer {
        interval: 1000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: clock.now = new Date()
    }

    Text {
        id: clockText

        anchors.centerIn: parent
        text: clock.now.toLocaleString(clock.locale, clock.pattern)
        color: Theme.textMuted
        font.family: Theme.sans
        font.pixelSize: 13
        font.weight: Font.DemiBold
        renderType: Text.NativeRendering
    }

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: clickArea.containsMouse ? Theme.hover : "transparent"

        Behavior on color {
            ColorAnimation { duration: Theme.reduceMotion ? 0 : 130 }
        }
    }

    MouseArea {
        id: clickArea

        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
    }

    // -- Calendar cells ------------------------------------------------------

    function daysInMonth(year, month) {
        return new Date(year, month + 1, 0).getDate();
    }

    function firstOffset(year, month) {
        const day = new Date(year, month, 1).getDay();
        return mondayFirst ? (day + 6) % 7 : day;
    }

    function isoWeek(date) {
        const target = new Date(date.getFullYear(), date.getMonth(), date.getDate());
        const dayNr = (target.getDay() + 6) % 7;
        target.setDate(target.getDate() - dayNr + 3);
        const firstThursday = new Date(target.getFullYear(), 0, 4);
        const firstDayNr = (firstThursday.getDay() + 6) % 7;
        firstThursday.setDate(firstThursday.getDate() - firstDayNr + 3);
        return 1 + Math.round((target - firstThursday) / 604800000);
    }

    // Rows of {week, days}; a zero day is a leading/trailing blank. With Monday
    // first each row also carries its ISO week number.
    readonly property var calendarCells: {
        const year = now.getFullYear();
        const month = now.getMonth();
        const perRow = mondayFirst ? 8 : 7;
        const lead = firstOffset(year, month);
        const total = daysInMonth(year, month);
        const rows = [];
        let row = { week: "", days: [] };
        for (let blank = 0; blank < lead && rows.length === 0; ++blank)
            row.days.push(0);
        for (let day = 1; day <= total; ++day) {
            row.days.push(day);
            if (row.days.length === perRow) {
                rows.push(row);
                row = { week: "", days: [] };
            }
        }
        if (row.days.length > 0) {
            while (row.days.length < perRow)
                row.days.push(0);
            rows.push(row);
        }
        if (mondayFirst) {
            for (const filled of rows)
                filled.week = String(isoWeek(new Date(year, month, filled.days[1] || 1)));
        }
        return rows;
    }

    readonly property string calendarTitle: new Date(
        now.getFullYear(), now.getMonth(), 1).toLocaleDateString(locale, "MMMM yyyy")

    function sameDay(cell) {
        return cell > 0 && cell === now.getDate();
    }

    Controls.Popup {
        id: calendarPopup

        y: clock.height + 6
        x: {
            const host = clock.Window.window;
            const raw = clock.width / 2 - width / 2 + clock.x;
            if (!host)
                return Math.max(4, raw);
            return Math.min(Math.max(4, raw), host.width - width - 4);
        }
        padding: 12
        visible: clickArea.containsMouse

        background: Rectangle {
            color: Theme.bodyRaised
            radius: 10
            border.color: Theme.frameOuter
            border.width: 1
        }

        contentItem: Column {
            spacing: 6

            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: clock.calendarTitle
                color: Theme.text
                font.family: Theme.sans
                font.pixelSize: 12
                font.weight: Font.DemiBold
                renderType: Text.NativeRendering
            }

            Grid {
                columns: clock.mondayFirst ? 8 : 7
                columnSpacing: 9
                rowSpacing: 3

                Repeater {
                    model: clock.mondayFirst
                        ? ["W", "Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
                        : ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]

                    delegate: Text {
                        required property var modelData

                        text: modelData
                        color: Theme.textMuted
                        font.family: Theme.sans
                        font.pixelSize: 10
                        renderType: Text.NativeRendering
                    }
                }

                Repeater {
                    model: clock.calendarCells

                    delegate: Repeater {
                        required property var modelData

                        model: clock.mondayFirst
                            ? [modelData.week].concat(modelData.days)
                            : modelData.days

                        delegate: Text {
                            required property var modelData
                            required property int index

                            text: index === 0 && clock.mondayFirst
                                ? modelData
                                : modelData > 0 ? String(modelData) : ""
                            color: index === 0 && clock.mondayFirst
                                ? Theme.textDisabled
                                : clock.sameDay(modelData) ? Theme.accent
                                : modelData > 0 ? Theme.text : "transparent"
                            font.family: Theme.sans
                            font.pixelSize: 11
                            font.weight: clock.sameDay(modelData)
                                ? Font.Bold : Font.Normal
                            horizontalAlignment: Text.AlignHCenter
                            renderType: Text.NativeRendering
                        }
                    }
                }
            }
        }
    }
}
