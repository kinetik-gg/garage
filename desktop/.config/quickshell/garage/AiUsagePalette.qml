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
    // LazyLoader that owns it every time it is reopened, so every open fetches
    // once. What is on screen before that answer arrives is the previous open's
    // payload out of PaletteCache, labelled as such.
    property bool loading: true
    property bool available: false
    property bool stale: false
    property var subscriptions: []
    property var today: null

    // When the payload on screen was fetched, for as long as it is the one
    // PaletteCache handed over rather than one this window fetched. Zero means
    // there was nothing cached to open with. See the header note below: quota
    // bars are worth showing immediately, but a bar drawn from a reading taken
    // an hour ago is not a bar anyone should plan a session around without
    // being told.
    property real seededAt: 0

    // The clock the reset lines are measured against, as a property so they are
    // bindings on it rather than one-shot reads of Date.now(). The payload above
    // is fetched once per open and never refreshed, which is the right policy
    // for a quota -- but "in 2h 40m" is a statement about now rather than about
    // the payload, and a panel left up on a second monitor would otherwise still
    // be claiming it half an hour later. Half a minute is finer than the minute
    // the line is printed to.
    property real now: Date.now()

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
            // Only a payload with something in it is worth keeping: replaying an
            // "unavailable" answer would have the next open report a missing
            // tokscale before it had looked for one. A failed parse falls
            // through to the catch and leaves the previous entry alone, which is
            // the same reasoning -- one bad run should not cost the panel the
            // last good reading.
            if (usage.available)
                PaletteCache.saveAiUsage({
                    stale: usage.stale,
                    subscriptions: usage.subscriptions,
                    today: usage.today
                });
        } catch (failure) {
            usage.available = false;
            usage.subscriptions = [];
            usage.today = null;
        }
        // Whatever the run said, it is this window's own answer now, so the
        // header stops attributing what is on screen to the cache.
        usage.seededAt = 0;
        usage.loading = false;
    }

    // Draw the last good payload while the helper runs. garage-ai-usage shells
    // out to tokscale and can take a second or more over it, and the panel is
    // opened to read a number off it -- an empty "Checking Tokscale…" pane for
    // data that has not moved since the last look is the wrong first frame.
    function restore() {
        const state = PaletteCache.aiUsageState;
        if (!state)
            return;
        usage.available = true;
        usage.stale = state.stale;
        usage.subscriptions = state.subscriptions;
        usage.today = state.today;
        usage.seededAt = PaletteCache.aiUsageSavedAt;
    }

    Component.onCompleted: usage.restore()

    // What the header says about where the figures came from. Nothing at all
    // once this window has its own fresh payload -- the common case, and a
    // panel that is telling the truth has no reason to say so.
    readonly property string sourceNote: {
        if (usage.seededAt > 0)
            return "Cached " + PaletteCache.formatAge(usage.seededAt, usage.now);
        return usage.stale ? "Cached" : "";
    }

    function formatPercent(value) {
        return (typeof value === "number") ? Math.round(value) + "%" : "—";
    }

    // tokscale's resets_at, verbatim out of the payload: garage-ai-usage's
    // --json mode passes the provider list straight through (build_json_output),
    // so this is whatever ISO-8601 string tokscale wrote -- with a Z, with a
    // numeric offset, or with no zone designator at all, all three of which the
    // script's own reset()/reset_days() handle.
    //
    // A naive timestamp is the one case where JS and the script disagree:
    // `new Date("2026-08-12T07:00:00")` is *local* time by ES2015, while
    // garage-ai-usage assumes UTC for the same string. The Z is appended here so
    // the palette and the bar cannot report two different reset moments for one
    // quota.
    function resetMoment(iso) {
        const text = String(iso).trim();
        const zoned = /(Z|[+-]\d{2}:?\d{2})$/.test(text);
        return new Date(zoned ? text : text + "Z");
    }

    // How long is left, at the granularity the number is actually useful at.
    //
    // This used to be a day count and nothing else, and it was wrong in the one
    // case that matters: a session quota resetting in a couple of hours ceils to
    // one day and read "resets in 1 day", which is a whole working session of
    // wrong. Inside 24 hours the answer is a clock time -- the thing you plan
    // around -- with the remaining hours and minutes after it, because a bare
    // "07:00" does not say whether that is soon or tomorrow morning. Past 24
    // hours the clock time stops meaning anything a week out, so day granularity
    // stays: "resets in 6 days".
    function formatReset(iso) {
        if (!iso)
            return "not reported";
        const at = usage.resetMoment(iso);
        if (isNaN(at.getTime()))
            return String(iso);

        const remaining = at.getTime() - usage.now;
        if (remaining <= 0)
            return "resets now";

        const clock = "resets " + Qt.formatDateTime(at, "HH:mm");
        if (remaining < 3600000)
            return clock + " · in " + Math.max(1, Math.ceil(remaining / 60000)) + "m";
        if (remaining < 86400000) {
            const hours = Math.floor(remaining / 3600000);
            const minutes = Math.floor((remaining % 3600000) / 60000);
            return clock + " · in " + hours + "h"
                + (minutes > 0 ? " " + minutes + "m" : "");
        }

        const days = Math.ceil(remaining / 86400000);
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

    // How tall the scrolling list may grow before it scrolls instead of the
    // window. The window is as tall as its content and the content is
    // open-ended -- a provider list plus a row per model used today -- so
    // without a ceiling a busy day is a panel taller than the output with its
    // total row below the bottom edge and no way to reach it. That is what the
    // Flickable is for; this is the height at which it starts doing its job.
    //
    // Measured off the output minus the fixed header this list scrolls under,
    // and minus a further allowance for Waybar: the anchor already sits below
    // the bar's exclusive zone, and its height is not something this window is
    // told. `restingTop` rather than the live top margin, which travels during
    // the entrance and would drag the ceiling with it.
    readonly property real bodyCeiling: {
        const target = usage.targetScreen();
        const chrome = header.implicitHeight + headerRule.implicitHeight
            + content.spacing * 2 + usage.contentMargin * 2;
        return Math.max(160, (target ? target.height : 1080) - 96
            - motion.restingTop - Theme.windowGutter - chrome);
    }

    screen: usage.targetScreen()
    implicitWidth: 360
    // Floored at 1px: a layer surface with no height is not one the
    // compositor can show, and the column's implicit height is zero for the
    // frame before its children are laid out.
    implicitHeight: Math.max(1, content.implicitHeight + usage.contentMargin * 2)
    color: "transparent"
    // OnDemand rather than Exclusive, the same choice as the notification and
    // control centres: a panel the user clicks into, not a modal.
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    // Top-to-bottom entrance, shared with every other palette. See PanelMotion.
    PanelMotion {
        id: motion
        onFinished: usage.dismissed()
    }

    function requestDismissal() {
        motion.dismiss();
    }

    // Top and right only. Anchoring the bottom as well made the panel as tall as
    // the output whatever was in it, so a short quota summary sat at the top of a
    // full-height sheet of glass.
    anchors {
        top: true
        right: true
    }

    // Overlay surfaces already begin below Waybar's exclusive zone, so the top
    // gutter is measured from there rather than from the top of the screen.
    margins.top: motion.surfaceTop
    margins.right: Theme.windowGutter

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

    // Advances `usage.now`, which is the only thing the reset lines re-read.
    // Nothing is fetched on this tick -- see the note on `now` above.
    Timer {
        interval: 30000
        repeat: true
        running: usage.visible
        onTriggered: usage.now = Date.now()
    }

    ContinuousRectangle {
        id: panel
        opacity: motion.opacity
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass: garage-ai-usage is one of the compositor's
        // glass layer namespaces, so the material is drawn beneath this
        // surface and painting a body here would cover it.
        color: Theme.panel
        borderWidth: 1
        borderColor: Theme.frameOuter

        // The body, over the glass and under everything else. Theme.panel is
        // transparent so the compositor's material shows through, and the
        // material alone is not a readable surface: over a bright window this
        // panel and its text wash out together. Declared before the content so
        // stacking order keeps it underneath without needing a z of its own.
        ContinuousRectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Theme.insetRadius(panel.radius, 1)
            power: Theme.cornerPower
            color: Theme.contentTint
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
            id: content
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: usage.contentMargin
            spacing: 10

            RowLayout {
                id: header
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
                // came from a cache rather than the subprocess that just ran --
                // either garage-ai-usage's own, the distinction --stale reports
                // on the waybar module, or PaletteCache while this open's run is
                // still in flight.
                Text {
                    visible: usage.sourceNote !== ""
                    text: usage.sourceNote
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 10
                    renderType: Text.NativeRendering
                }
            }

            MenuSeparator { id: headerRule }

            Flickable {
                Layout.fillWidth: true
                // As tall as what it holds, up to the ceiling. fillHeight here
                // asked for a share of a height the column does not have -- the
                // window is sized from this content rather than the other way
                // round since it stopped being full-output-height -- so the list
                // collapsed to nothing and the panel rendered as a title and a
                // rule over an empty body.
                Layout.preferredHeight: Math.min(body.implicitHeight, usage.bodyCeiling)
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
                                            id: quotaWell
                                            Layout.fillWidth: true
                                            implicitHeight: 6
                                            radius: height
                                            color: Theme.hoverStrong

                                            // One colour at two opacities, not
                                            // two colours: the well behind this
                                            // is Theme.hoverStrong, which is
                                            // this same foreground white at 9%.
                                            // The accent that used to fill this
                                            // implied a status the number does
                                            // not have -- every provider's bar
                                            // was the same blue whether it read
                                            // 4% or 96%.
                                            // The well by id rather than by
                                            // `parent`: ContinuousRectangle puts
                                            // its children in a plain content
                                            // Item, which fills it but has no
                                            // radius of its own -- so the fill
                                            // was being handed undefined for its
                                            // corners on every frame it was laid
                                            // out in.
                                            ContinuousRectangle {
                                                width: quotaWell.width * Math.max(0, Math.min(1,
                                                    (metricRow.modelData.remaining_percent || 0) / 100))
                                                height: quotaWell.height
                                                radius: quotaWell.radius
                                                color: Theme.text
                                                opacity: 0.9
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
                // Two lines tall, not half a screen. The spacers that used to
                // centre this vertically were sized by a full-output-height
                // panel; with the window measuring itself from this column they
                // would each claim a share of nothing, and the panel would be a
                // sheet of glass with a sentence somewhere in it.
                Layout.topMargin: 12
                Layout.bottomMargin: 12
                visible: !usage.available
                spacing: 8

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
            }
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: usage.dismissed()
    }
}
