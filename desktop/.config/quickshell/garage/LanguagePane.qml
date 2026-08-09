import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property var state: controller.snapshot.dateTime || ({})
    readonly property var region: controller.snapshot.region || ({})
    property date now: new Date()

    readonly property string chosenLocale: controller.preference("region", "locale", "")
    readonly property string timeFormat: controller.preference("region", "time_format", "24")
    readonly property string dateFormat: controller.preference("region", "date_format", "dmy")
    readonly property string firstDay: controller.preference("region", "first_day_of_week", "sunday")

    // The Qt spellings of the same two halves the helper writes as strftime for
    // the bar, so the clock above and the one in the corner cannot disagree.
    readonly property var timePatterns: ({ "12": "h:mm:ss AP", "24": "HH:mm:ss" })
    readonly property var dateKeys: ["dmy", "mdy", "iso"]
    readonly property var datePatterns: ({
        "dmy": "dddd, d MMMM yyyy",
        "mdy": "dddd, MMMM d, yyyy",
        "iso": "dddd, yyyy-MM-dd"
    })
    // Each order shown as today's date, so the choice reads as what it will
    // actually look like rather than as three abbreviations. Evaluated once
    // rather than off `now`: a model rebuilt every second drops the selection
    // out from under an open menu.
    readonly property var dateSamples: ["d MMM yyyy", "MMM d, yyyy", "yyyy-MM-dd"]
        .map(pattern => Qt.formatDate(new Date(), pattern))

    // Empty means no override, which resolves to whatever the system sets.
    readonly property string effectiveLocale: chosenLocale || (region.system || "")
    // What the running session is in, not what the next application will be in:
    // LANG is read once at startup, so these part company the moment the
    // setting changes and only a new login brings them back together.
    readonly property bool localeLive: effectiveLocale === (region.active || "")

    readonly property var localeNames: region.locales || []
    readonly property var localeLabels: ["System Default"].concat(localeNames)

    function localeIndex() {
        const index = pane.localeNames.indexOf(pane.chosenLocale);
        return index < 0 ? 0 : index + 1;
    }

    function timezoneIndex() {
        const zones = state.timezones || [];
        const index = zones.indexOf(state.timezone || "");
        return Math.max(0, index);
    }

    function applyAction(name, value) {
        controller.action(name, value);
        delayedRefresh.restart();
    }

    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: pane.now = new Date()
    }

    Timer {
        id: delayedRefresh
        interval: 2500
        onTriggered: pane.controller.refresh()
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        ColumnLayout {
            Layout.fillWidth: true
            Layout.topMargin: 6
            Layout.bottomMargin: 4
            spacing: 5

            Text {
                Layout.fillWidth: true
                text: Qt.formatTime(pane.now, pane.timePatterns[pane.timeFormat])
                color: Theme.text
                font.family: Theme.sans
                font.pixelSize: 38
                font.weight: Font.Light
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }

            Text {
                Layout.fillWidth: true
                text: Qt.formatDate(pane.now, pane.datePatterns[pane.dateFormat])
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 13
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }
        }

        SettingsGroup {
            title: "LANGUAGE"

            SettingsRow {
                title: "Language"
                description: "Sets LANG for this user. The system locale ("
                    + (pane.region.system || "unknown")
                    + ") needs administrator access and is left alone."
                SettingsCombo {
                    implicitWidth: 230
                    maxPopupHeight: 280
                    model: pane.localeLabels
                    currentIndex: pane.localeIndex()
                    onActivated: index => pane.controller.setPreference(
                        "region", "locale", index === 0 ? "" : pane.localeNames[index - 1])
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            // Said plainly, because a language that appears to do nothing is
            // worse than no setting at all. Only what has yet to start can be
            // moved; everything on screen was handed its LANG at login.
            Text {
                Layout.fillWidth: true
                text: pane.localeLive
                    ? "In effect. This session is running in " + pane.effectiveLocale + "."
                    : "Applications opened from now on start in "
                        + (pane.region.session || pane.effectiveLocale)
                        + ". The shell, the menu bar and every open window stay in "
                        + (pane.region.active || "the previous language")
                        + " until you log out and back in."
                color: pane.localeLive ? Theme.textMuted : Theme.text
                font.family: Theme.sans
                font.pixelSize: 11
                wrapMode: Text.WordWrap
                renderType: Text.NativeRendering
            }

            // One installed locale is not a choice. Naming the tool is more use
            // than an empty menu, since nothing in this pane can add one.
            Text {
                Layout.fillWidth: true
                visible: pane.localeNames.length < 2
                text: "Only one locale is generated on this system. Add more by "
                    + "uncommenting them in /etc/locale.gen and running locale-gen."
                color: Theme.textDisabled
                font.family: Theme.sans
                font.pixelSize: 11
                wrapMode: Text.WordWrap
                renderType: Text.NativeRendering
            }
        }

        SettingsGroup {
            title: "FORMATS"

            SettingsRow {
                title: "Time Format"
                description: "Used by the menu bar clock and the time above."
                SettingsSegmented {
                    model: ["12-Hour", "24-Hour"]
                    currentIndex: pane.timeFormat === "12" ? 0 : 1
                    onActivated: index => pane.controller.setPreference(
                        "region", "time_format", index === 0 ? "12" : "24")
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Date Format"
                description: "Order the menu bar clock writes the date in."
                SettingsCombo {
                    implicitWidth: 190
                    model: pane.dateSamples
                    currentIndex: Math.max(0, pane.dateKeys.indexOf(pane.dateFormat))
                    onActivated: index => pane.controller.setPreference(
                        "region", "date_format", pane.dateKeys[index])
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "First Day of Week"
                description: "Used by the calendar in the menu bar clock's tooltip."
                SettingsSegmented {
                    model: ["Sunday", "Monday"]
                    currentIndex: pane.firstDay === "monday" ? 1 : 0
                    onActivated: index => pane.controller.setPreference(
                        "region", "first_day_of_week", index === 0 ? "sunday" : "monday")
                }
            }
        }

        SettingsGroup {
            title: "DATE & TIME"

            SettingsRow {
                title: "Set Time Automatically"
                description: pane.state.synchronized
                    ? "Clock synchronized with the network."
                    : "Network clock synchronization is not yet confirmed."
                SettingsSwitch {
                    checked: Boolean(pane.state.ntp)
                    onToggled: value => pane.applyAction("datetime.ntp", value)
                }
            }

            SettingsRow {
                title: "Time Zone"
                description: pane.state.timezone || "Unavailable"
                SettingsCombo {
                    implicitWidth: 230
                    maxPopupHeight: 280
                    model: pane.state.timezones || []
                    currentIndex: pane.timezoneIndex()
                    onActivated: index => pane.applyAction("datetime.timezone", model[index])
                }
            }
        }

        Text {
            Layout.fillWidth: true
            text: "Changing system time settings may require authentication."
            color: Theme.textDisabled
            font.family: Theme.sans
            font.pixelSize: 10
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            renderType: Text.NativeRendering
        }

        Item { Layout.preferredHeight: 20 }
    }
}
