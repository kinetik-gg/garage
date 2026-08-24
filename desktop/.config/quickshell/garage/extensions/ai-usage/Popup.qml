pragma ComponentBehavior: Bound

import Quickshell.Io
import QtQuick
import QtQuick.Layouts
import "../.." as Garage

// Extension-local AI detail. WidgetPopover supplies the surface and dismissal;
// this item owns only the subscription and daily-usage content within it.
Item {
    id: usage

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    readonly property var theme: bar.theme
    property bool loading: true
    property bool available: false
    property bool stale: false
    property var subscriptions: []
    property var today: null
    property real now: Date.now()
    property string refreshError: ""

    implicitWidth: 360
    implicitHeight: Math.max(150, content.implicitHeight + 24)

    function loadPayload(text) {
        let payload;
        try {
            payload = JSON.parse(String(text));
        } catch (error) {
            failRefresh("Invalid response from garage-ai-usage");
            return;
        }
        if (!payload || payload.available !== true) {
            failRefresh("Tokscale usage is unavailable");
            return;
        }

        const nextSubscriptions = Array.isArray(payload.subscriptions)
            ? payload.subscriptions : [];
        available = true;
        stale = payload.stale === true;
        subscriptions = nextSubscriptions;
        today = payload.today && typeof payload.today === "object"
            ? payload.today : null;
        refreshError = "";
        loading = false;
    }

    function failRefresh(message) {
        refreshError = String(message);
        loading = false;
        available = false;
        subscriptions = [];
        today = null;
    }

    readonly property string sourceNote: stale ? "Cached by collector" : ""

    function formatPercent(value) {
        return typeof value === "number" ? Math.round(value) + "%" : "—";
    }

    function resetMoment(iso) {
        const text = String(iso || "").trim();
        const zoned = /(Z|[+-]\d{2}:?\d{2})$/.test(text);
        return new Date(zoned ? text : text + "Z");
    }

    function formatReset(iso) {
        if (!iso)
            return "Reset not reported";
        const at = resetMoment(iso);
        if (isNaN(at.getTime()))
            return String(iso);
        const remaining = at.getTime() - now;
        if (remaining <= 0)
            return "Resets now";
        const clock = "Resets " + Qt.formatDateTime(at, "HH:mm");
        if (remaining < 3600000)
            return clock + " · in " + Math.max(1,
                Math.ceil(remaining / 60000)) + "m";
        if (remaining < 86400000) {
            const hours = Math.floor(remaining / 3600000);
            const minutes = Math.floor((remaining % 3600000) / 60000);
            return clock + " · in " + hours + "h"
                + (minutes > 0 ? " " + minutes + "m" : "");
        }
        const days = Math.ceil(remaining / 86400000);
        return "Resets in " + days + (days === 1 ? " day" : " days");
    }

    function numberField(object, camel, snake) {
        if (!object)
            return 0;
        const value = object[camel] !== undefined
            ? object[camel] : object[snake];
        return Number(value) || 0;
    }

    function entryTokens(entry) {
        return numberField(entry, "input", "input_tokens")
            + numberField(entry, "output", "output_tokens")
            + numberField(entry, "cacheRead", "cache_read")
            + numberField(entry, "cacheWrite", "cache_write");
    }

    function formatTokens(value) {
        const count = Number(value) || 0;
        if (count >= 1000000)
            return (count / 1000000).toFixed(1).replace(".0", "") + "M";
        if (count >= 1000)
            return (count / 1000).toFixed(1).replace(".0", "") + "K";
        return String(Math.round(count));
    }

    function formatCost(value) {
        return "$" + (Number(value) || 0).toFixed(2);
    }

    readonly property var todayEntries: today && Array.isArray(today.entries)
        ? today.entries : []
    readonly property real todayTotal: today
        ? numberField(today, "totalCost", "total_cost") : 0

    Process {
        id: usageProcess
        command: [String(usage.services.paths.aiUsage), "--json"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: usage.loadPayload(text)
        }
        stderr: StdioCollector {}
        onExited: exitCode => {
            if (exitCode !== 0 && usage.loading)
                usage.failRefresh("garage-ai-usage exited (" + exitCode + ")");
        }
    }

    Timer {
        interval: 30000
        repeat: true
        running: usage.visible
        onTriggered: usage.now = Date.now()
    }

    ColumnLayout {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 12
        spacing: 10

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Text {
                text: "AI Usage"
                color: usage.theme.text
                font.family: usage.theme.sans
                font.pixelSize: 17
                font.weight: Font.DemiBold
                renderType: Text.NativeRendering
            }

            Item { Layout.fillWidth: true }

            Text {
                visible: usage.sourceNote !== ""
                text: usage.sourceNote
                color: usage.theme.textMuted
                font.family: usage.theme.sans
                font.pixelSize: 10
                renderType: Text.NativeRendering
            }
        }

        Garage.MenuSeparator { Layout.fillWidth: true }

        Flickable {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(detailBody.implicitHeight, 390)
            contentWidth: width
            contentHeight: detailBody.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            visible: usage.available

            ColumnLayout {
                id: detailBody
                width: parent.width
                spacing: 16

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    visible: usage.subscriptions.length > 0

                    Text {
                        text: "SUBSCRIPTIONS"
                        color: usage.theme.textMuted
                        font.family: usage.theme.sans
                        font.pixelSize: 11
                        font.weight: Font.DemiBold
                        renderType: Text.NativeRendering
                    }

                    Repeater {
                        model: usage.subscriptions

                        ColumnLayout {
                            id: providerCard
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 7

                            // Repeater exposes an object row as a QVariant-backed
                            // model object, so its nested list is not a JS Array
                            // even though JSON.parse produced one. Feed that list
                            // directly to the nested Repeater.
                            readonly property var metrics: providerCard.modelData.metrics || []

                            RowLayout {
                                Layout.fillWidth: true
                                Text {
                                    text: providerCard.modelData.provider || "Provider"
                                    color: usage.theme.text
                                    font.family: usage.theme.sans
                                    font.pixelSize: 13
                                    font.weight: Font.Medium
                                    renderType: Text.NativeRendering
                                }
                                Item { Layout.fillWidth: true }
                                Text {
                                    text: providerCard.modelData.plan || ""
                                    color: usage.theme.textMuted
                                    font.family: usage.theme.sans
                                    font.pixelSize: 11
                                    renderType: Text.NativeRendering
                                }
                            }

                            Repeater {
                                model: providerCard.metrics

                                ColumnLayout {
                                    id: metricRow
                                    required property var modelData
                                    Layout.fillWidth: true
                                    spacing: 3

                                    RowLayout {
                                        Layout.fillWidth: true
                                        Text {
                                            Layout.fillWidth: true
                                            text: metricRow.modelData.label || "Quota"
                                            color: usage.theme.textMuted
                                            font.family: usage.theme.sans
                                            font.pixelSize: 11
                                            elide: Text.ElideRight
                                            renderType: Text.NativeRendering
                                        }
                                        Text {
                                            text: usage.formatPercent(
                                                metricRow.modelData.remaining_percent) + " left"
                                            color: usage.theme.text
                                            font.family: usage.theme.mono
                                            font.pixelSize: 11
                                            renderType: Text.NativeRendering
                                        }
                                    }

                                    Garage.ContinuousRectangle {
                                        id: quotaWell
                                        Layout.fillWidth: true
                                        implicitHeight: 6
                                        radius: height
                                        color: usage.theme.hoverStrong

                                        Garage.ContinuousRectangle {
                                            width: quotaWell.width * Math.max(0,
                                                Math.min(1, Number(metricRow.modelData.remaining_percent) / 100))
                                            height: quotaWell.height
                                            radius: quotaWell.radius
                                            color: usage.theme.text
                                            opacity: 0.9
                                        }
                                    }

                                    Text {
                                        Layout.fillWidth: true
                                        text: usage.formatReset(metricRow.modelData.resets_at)
                                        color: usage.theme.textDisabled
                                        font.family: usage.theme.sans
                                        font.pixelSize: 10
                                        renderType: Text.NativeRendering
                                    }
                                }
                            }
                        }
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    Text {
                        text: "TODAY"
                        color: usage.theme.textMuted
                        font.family: usage.theme.sans
                        font.pixelSize: 11
                        font.weight: Font.DemiBold
                        renderType: Text.NativeRendering
                    }

                    Repeater {
                        model: usage.todayEntries

                        RowLayout {
                            id: entryRow
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 8

                            Text {
                                Layout.fillWidth: true
                                text: entryRow.modelData.model || "Unknown model"
                                color: usage.theme.text
                                font.family: usage.theme.sans
                                font.pixelSize: 12
                                elide: Text.ElideRight
                                renderType: Text.NativeRendering
                            }
                            Text {
                                text: usage.formatTokens(usage.entryTokens(entryRow.modelData))
                                color: usage.theme.textMuted
                                font.family: usage.theme.mono
                                font.pixelSize: 11
                                renderType: Text.NativeRendering
                            }
                            Text {
                                Layout.preferredWidth: 54
                                horizontalAlignment: Text.AlignRight
                                text: usage.formatCost(entryRow.modelData.cost)
                                color: usage.theme.text
                                font.family: usage.theme.mono
                                font.pixelSize: 11
                                renderType: Text.NativeRendering
                            }
                        }
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: usage.todayEntries.length === 0
                        text: "No usage logged today."
                        color: usage.theme.textDisabled
                        font.family: usage.theme.sans
                        font.pixelSize: 11
                        horizontalAlignment: Text.AlignHCenter
                        renderType: Text.NativeRendering
                    }

                    Garage.MenuSeparator {
                        Layout.fillWidth: true
                        visible: usage.todayEntries.length > 0
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        visible: usage.todayEntries.length > 0
                        Text {
                            Layout.fillWidth: true
                            text: "Total"
                            color: usage.theme.textMuted
                            font.family: usage.theme.sans
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                            renderType: Text.NativeRendering
                        }
                        Text {
                            text: usage.formatCost(usage.todayTotal)
                            color: usage.theme.text
                            font.family: usage.theme.mono
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                            renderType: Text.NativeRendering
                        }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.topMargin: 14
            Layout.bottomMargin: 14
            visible: !usage.available
            spacing: 8

            Text {
                Layout.fillWidth: true
                text: usage.loading ? "Checking Tokscale…"
                    : "Tokscale usage unavailable"
                color: usage.theme.textMuted
                font.family: usage.theme.sans
                font.pixelSize: 13
                font.weight: Font.Medium
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }

            Text {
                Layout.fillWidth: true
                visible: !usage.loading
                text: usage.refreshError !== "" ? usage.refreshError
                    : "Install Tokscale to see subscription usage here."
                color: usage.theme.textDisabled
                font.family: usage.theme.sans
                font.pixelSize: 11
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }
        }
    }
}
