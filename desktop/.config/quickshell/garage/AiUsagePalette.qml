import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts

// The AI usage palette: subscription quotas and today's token spend, read from
// garage-ai-usage --json. Mirrors ControlCenterPalette's contract (see that
// file for the rationale behind OnDemand keyboard focus, the overlay layer and
// the glass body) but full height like the notification centre next to it --
// a provider list plus a per-model table is open-ended content, not a fixed
// handful of tiles, so the window must not try to size itself to it.
//
// garage-ai-usage is optional: it reports {"available": false} on its own
// when the tokscale CLI cannot be found (see that script's --probe mode), so
// this palette does not need a second process just to decide whether to show
// the empty state.
PanelWindow {
    id: usage

    required property string targetScreenName

    signal dismissed()

    readonly property int contentMargin: 12
    readonly property string helper: Quickshell.env("HOME") + "/.local/bin/garage-ai-usage"

    // True only until the one-shot process answers. There is nothing to
    // refresh after that -- the palette is destroyed and recreated by the
    // LazyLoader that owns it every time it is reopened, which is what keeps
    // this data from ever going stale on screen.
    property bool loading: true
    property bool available: false
    property bool stale: false
    property var subscriptions: []
    property var today: null

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === usage.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    function loadPayload(text) {
        try {
            const payload = JSON.parse(text);
            usage.available = !!payload.available;
            usage.stale = !!payload.stale;
            usage.subscriptions = payload.subscriptions || [];
            usage.today = payload.today || null;
        } catch (failure) {
            usage.available = false;
            usage.subscriptions = [];
            usage.today = null;
        }
        usage.loading = false;
    }

    function formatPercent(value) {
        return (typeof value === "number") ? Math.round(value) + "%" : "—";
    }

    // A day count rather than a clock time: the popover has no room for a
    // timezone-qualified timestamp, and "resets in 6 days" is the number a
    // quota bar actually gets read for. garage-ai-usage's own --bar tooltip
    // makes the same call (see reset_days() in that script).
    function formatReset(iso) {
        if (!iso)
            return "not reported";
        const at = new Date(iso);
        if (isNaN(at.getTime()))
            return String(iso);
        const days = Math.max(0, Math.ceil((at.getTime() - Date.now()) / 86400000));
        if (days <= 0)
            return "resets today";
        return "resets in " + days + (days === 1 ? " day" : " days");
    }

    function formatTokens(count) {
        const value = Number(count) || 0;
        if (value >= 1000000)
            return (value / 1000000).toFixed(1).replace(".0", "") + "M";
        if (value >= 1000)
            return (value / 1000).toFixed(1).replace(".0", "") + "K";
        return String(Math.round(value));
    }

    function formatCost(value) {
        return "$" + (Number(value) || 0).toFixed(2);
    }

    // Every token tokscale counted for the entry, cache included -- the same
    // total the entry's own cost was computed from, so the two columns agree.
    function entryTokens(entry) {
        return (entry.input || 0) + (entry.output || 0)
            + (entry.cacheRead || 0) + (entry.cacheWrite || 0);
    }

    screen: usage.targetScreen()
    implicitWidth: 360
    color: "transparent"
    // OnDemand rather than Exclusive, the same choice as the notification and
    // control centres: a panel the user clicks into, not a modal.
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    anchors {
        top: true
        right: true
        bottom: true
    }

    // Overlay surfaces already begin below Waybar's exclusive zone, so the top
    // gutter is measured from there rather than from the top of the screen.
    margins.top: Theme.windowGutter
    margins.right: Theme.windowGutter
    margins.bottom: Theme.windowGutter

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "garage-ai-usage"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    // One shot, on open: the palette is recreated by its LazyLoader every
    // time it is shown, so `running: true` here is the whole refresh policy --
    // there is no timer to leave running behind after the panel closes.
    Process {
        id: usageProcess
        command: [usage.helper, "--json"]
        running: true
        stdout: StdioCollector { onStreamFinished: usage.loadPayload(text) }
        stderr: StdioCollector {}
    }

    ContinuousRectangle {
        id: panel
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass: garage-ai-usage is one of the compositor's
        // glass layer namespaces, so the material is drawn beneath this
        // surface and painting a body here would cover it.
        color: Theme.panel
        borderWidth: 1
        borderColor: Theme.frameOuter

        NumberAnimation on opacity {
            from: 0
            to: 1
            duration: 130
            easing.type: Easing.OutCubic
        }

        // Inner hairline one inset px in from the outer one, the double frame
        // every other panel in the shell draws.
        ContinuousRectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Theme.insetRadius(panel.radius, 1)
            power: Theme.cornerPower
            borderWidth: 1
            borderColor: Theme.frameInner
        }

        MouseArea {
            anchors.fill: parent
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: usage.contentMargin
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                Text {
                    text: "AI Usage"
                    color: Theme.text
                    font.family: Theme.sans
                    font.pixelSize: 17
                    font.weight: Font.DemiBold
                    renderType: Text.NativeRendering
                }

                Item { Layout.fillWidth: true }

                // Cached data is still today's, but it is worth saying it
                // came from garage-ai-usage's own cache rather than the
                // subprocess that just ran -- the same distinction --stale
                // reports on the waybar module.
                Text {
                    visible: usage.stale
                    text: "Cached"
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 10
                    renderType: Text.NativeRendering
                }
            }

            MenuSeparator {}

            Flickable {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: width
                contentHeight: body.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                visible: usage.available

                ColumnLayout {
                    id: body
                    width: parent.width
                    spacing: 16

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        visible: usage.subscriptions.length > 0

                        Text {
                            text: "SUBSCRIPTIONS"
                            color: Theme.textMuted
                            font.family: Theme.sans
                            font.pixelSize: 11
                            font.weight: Font.DemiBold
                            renderType: Text.NativeRendering
                        }

                        // One card per provider (Codex, Claude, Copilot, …),
                        // each with one bar per metric that reported a
                        // remaining_percent -- reset_credits, credit_status
                        // and spend_control ride along in the payload but
                        // are not quota bars, so they are left alone here.
                        Repeater {
                            model: usage.subscriptions

                            ColumnLayout {
                                id: providerCard
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 6

                                readonly property var metrics: (providerCard.modelData.metrics || [])
                                    .filter(entry => typeof entry.remaining_percent === "number")

                                RowLayout {
                                    Layout.fillWidth: true
                                    Text {
                                        text: providerCard.modelData.provider || "Provider"
                                        color: Theme.text
                                        font.family: Theme.sans
                                        font.pixelSize: 13
                                        font.weight: Font.Medium
                                        renderType: Text.NativeRendering
                                    }
                                    Item { Layout.fillWidth: true }
                                    Text {
                                        text: providerCard.modelData.plan || ""
                                        color: Theme.textMuted
                                        font.family: Theme.sans
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
                                                text: metricRow.modelData.label || ""
                                                color: Theme.textMuted
                                                font.family: Theme.sans
                                                font.pixelSize: 11
                                                elide: Text.ElideRight
                                                renderType: Text.NativeRendering
                                            }
                                            Text {
                                                text: usage.formatPercent(metricRow.modelData.remaining_percent) + " left"
                                                color: Theme.text
                                                font.family: Theme.mono
                                                font.pixelSize: 11
                                                renderType: Text.NativeRendering
                                            }
                                        }

                                        ContinuousRectangle {
                                            Layout.fillWidth: true
                                            implicitHeight: 6
                                            radius: height
                                            color: Theme.hoverStrong

                                            ContinuousRectangle {
                                                width: parent.width * Math.max(0, Math.min(1,
                                                    (metricRow.modelData.remaining_percent || 0) / 100))
                                                height: parent.height
                                                radius: parent.radius
                                                color: Theme.accent
                                            }
                                        }

                                        Text {
                                            Layout.fillWidth: true
                                            text: usage.formatReset(metricRow.modelData.resets_at)
                                            color: Theme.textDisabled
                                            font.family: Theme.sans
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
                        visible: usage.today !== null && (usage.today.entries || []).length > 0

                        Text {
                            text: "TODAY"
                            color: Theme.textMuted
                            font.family: Theme.sans
                            font.pixelSize: 11
                            font.weight: Font.DemiBold
                            renderType: Text.NativeRendering
                        }

                        Repeater {
                            model: usage.today ? (usage.today.entries || []) : []

                            RowLayout {
                                id: entryRow
                                required property var modelData
                                Layout.fillWidth: true
                                spacing: 8

                                Text {
                                    Layout.fillWidth: true
                                    text: entryRow.modelData.model || "Unknown model"
                                    color: Theme.text
                                    font.family: Theme.sans
                                    font.pixelSize: 12
                                    elide: Text.ElideRight
                                    renderType: Text.NativeRendering
                                }

                                Text {
                                    text: usage.formatTokens(usage.entryTokens(entryRow.modelData))
                                    color: Theme.textMuted
                                    font.family: Theme.mono
                                    font.pixelSize: 11
                                    renderType: Text.NativeRendering
                                }

                                Text {
                                    Layout.preferredWidth: 54
                                    horizontalAlignment: Text.AlignRight
                                    text: usage.formatCost(entryRow.modelData.cost)
                                    color: Theme.text
                                    font.family: Theme.mono
                                    font.pixelSize: 11
                                    renderType: Text.NativeRendering
                                }
                            }
                        }

                        MenuSeparator { Layout.topMargin: 4 }

                        RowLayout {
                            Layout.fillWidth: true
                            Text {
                                Layout.fillWidth: true
                                text: "Total"
                                color: Theme.textMuted
                                font.family: Theme.sans
                                font.pixelSize: 12
                                font.weight: Font.DemiBold
                                renderType: Text.NativeRendering
                            }
                            Text {
                                text: usage.formatCost(usage.today ? usage.today.totalCost : 0)
                                color: Theme.text
                                font.family: Theme.mono
                                font.pixelSize: 12
                                font.weight: Font.DemiBold
                                renderType: Text.NativeRendering
                            }
                        }
                    }

                    // Tokscale is present and answering, but the day's log is
                    // empty (or --today itself failed and today stayed
                    // null) -- worth a line rather than a blank gap under the
                    // subscriptions, which otherwise reads as a loading pane
                    // that never finished.
                    Text {
                        Layout.fillWidth: true
                        visible: usage.today === null || (usage.today.entries || []).length === 0
                        text: "No usage logged today."
                        color: Theme.textDisabled
                        font.family: Theme.sans
                        font.pixelSize: 11
                        horizontalAlignment: Text.AlignHCenter
                        renderType: Text.NativeRendering
                    }

                    Item { Layout.preferredHeight: 10 }
                }
            }

            // The CLI-absent and empty-payload cases both collapse to
            // available === false (see build_json_output in garage-ai-usage),
            // so one empty state covers a missing tokscale install and a
            // provider list tokscale returned nothing usable for.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: !usage.available
                spacing: 8

                Item { Layout.fillHeight: true }

                Text {
                    Layout.fillWidth: true
                    text: usage.loading ? "Checking Tokscale…" : "Tokscale usage unavailable"
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 13
                    font.weight: Font.Medium
                    horizontalAlignment: Text.AlignHCenter
                    renderType: Text.NativeRendering
                }

                Text {
                    Layout.fillWidth: true
                    visible: !usage.loading
                    text: "Install tokscale (~/.local/share/tokscale) or add it to PATH to see subscription usage here."
                    color: Theme.textDisabled
                    font.family: Theme.sans
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                    renderType: Text.NativeRendering
                }

                Item { Layout.fillHeight: true }
            }
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: usage.dismissed()
    }
}
