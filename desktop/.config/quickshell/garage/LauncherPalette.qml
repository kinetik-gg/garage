import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts
import "LauncherExtras.js" as LauncherExtras

// A layer surface, like every other transient palette in the shell, and unlike
// the FloatingWindow this used to be. A toplevel is dismissed by losing focus,
// and under focus-follows-mouse merely moving the pointer across another window
// took that focus away -- so the launcher closed on a cursor twitch. A layer
// surface has no such notion: it is dismissed by a click outside, by Escape, or
// by the keybind toggling the loader, and by nothing else.
//
// The click outside is caught by DismissCatcher, a surface the shell maps under
// this one, rather than by a focus grab here. This compositor does not
// implement hyprland-focus-grab -- the grab that used to be at the foot of this
// file never fired once -- so the launcher could only be closed from the
// keyboard until the catcher was added.
PaletteSurface {
    id: launcher
    surfaceNamespace: "garage-launcher-host"
    escapeEnabled: false
    // The launcher is a monitor-level affordance, not a popover belonging to
    // the bar edge that opened it. Keep it horizontally centred and place its
    // top edge one third down the selected monitor.
    edge: "top"
    targetAnchor: -1
    // A top-docked bar contributes an exclusive zone before layer-shell applies
    // this margin. Remove that zone so the launcher still lands at the same
    // monitor-relative coordinate on every bar edge.
    surfaceOffset: effectiveScreen ? Math.max(Theme.windowGutter,
        Math.round(effectiveScreen.height / 3)
            - (BarState.position === "top" ? BarState.thickness : 0))
        : Theme.windowGutter

    signal sessionActionRequested(string action)
    signal shellActionRequested(string action)

    // The monitor the launcher was asked for on, resolved by the shell at the
    // moment of the keypress and held as a name. Deliberately not read live from
    // Hyprland's focused monitor: under focus-follows-mouse that changes as soon
    // as the pointer crosses a screen edge, and an open launcher that jumps to
    // another monitor when the cursor drifts is the bug this file is fixing
    // wearing a different hat.
    property string query: ""
    property int selected: 0
    // "clip" is the dedicated Super+Shift+V source. The default mode retains
    // every existing launcher source and also recognises an explicit `clip`
    // query; only the dedicated mode interprets the whole field as a filter.
    property string initialMode: "default"
    readonly property bool clipboardMode: initialMode === "clip"
    // Desktop id of the browser to hand web searches to. Empty means none was
    // found, which the launcher has to say rather than silently doing nothing.
    property string browserId: ""
    property bool browserResolved: false
    // Owned by shell.qml because the idle-inhibiting surface must outlive this
    // palette. It is read here only to name the toggle's next state.
    property bool caffeine: false

    readonly property real rowHeight: 52
    readonly property int maxRows: 8
    // Eight visible rows plus a small offscreen buffer. Every query source is
    // sliced to this fixed model size before the string roles are rewritten.
    readonly property int maxResultRows: launcher.maxRows + 3
    readonly property real fieldHeight: 34
    readonly property real contentMargin: 14
    readonly property real listGap: 8

    implicitWidth: 640

    // Published once after the model and its parallel action array agree. The
    // visible panel and its glass backing both derive their result height from
    // this completed rebuild rather than from an intermediate model state.
    property int rowCount: 0
    property int pendingRowCount: 0
    property int geometryWaits: 0
    readonly property bool listing: launcher.rowCount > 0
    readonly property int visibleRows: Math.min(launcher.rowCount, launcher.maxRows)
    readonly property int pendingVisibleRows:
        Math.min(launcher.pendingRowCount, launcher.maxRows)
    readonly property int renderRows:
        Math.max(launcher.visibleRows, launcher.pendingVisibleRows)

    // The keyboard/content surface never changes size while the launcher is
    // open. Its changing height was the last geometry dependency shared by the
    // field and the result list: anchors fix their origin inside the surface,
    // but do not make the surface resize-free when a query changes row count.
    //
    // Fixing the surface itself at the largest panel height removes those
    // configures. The panel and its input mask remain content-sized inside it;
    // the separately mapped glassSurface below supplies the material without
    // turning the transparent remainder of this surface into a blurred slab.
    readonly property real surfaceHeight: launcher.contentMargin * 2
        + launcher.fieldHeight + launcher.listGap
        + launcher.maxRows * launcher.rowHeight
    readonly property real bodyHeight:
        Math.max(1, body.implicitHeight + launcher.contentMargin * 2)
    readonly property real contentHeight: noBrowser.visible
        ? Math.max(launcher.bodyHeight, 150 + launcher.contentMargin * 2)
        : launcher.bodyHeight
    implicitHeight: launcher.surfaceHeight
    mask: Region { item: panel }

    function requestDismissal() {
        launcher.dismissSurface();
    }
    // The fixed host is deliberately not a glass namespace. The content-sized
    // garage-launcher-glass surface below owns the material; keeping the two
    // names distinct also lets an older, already-running shell retain its legacy
    // garage-launcher glass rule while this file waits for a safe reload.
    // OnDemand, not Exclusive. An exclusive layer keyboard is held at the
    // protocol level no matter where the pointer goes, which takes every
    // keystroke in the session for as long as the launcher is up. On demand is
    // enough: the compositor hands this surface the keyboard as it maps, which
    // is what makes the query field typeable the moment the bind is pressed and
    // Escape heard without a click first. Leave this alone -- typing here works
    // today, and Exclusive is what it was before.

    // Kinetik Glass draws against a layer surface's complete logical box; it
    // cannot use the alpha of this fixed-height host to discover the shorter
    // panel inside it. Give the material its own content-sized, inputless
    // surface immediately below the overlay instead. It can resize as results
    // change because it carries no field, delegates, or layout to disturb.
    PanelWindow {
        id: glassSurface

        screen: launcher.effectiveScreen
        implicitWidth: launcher.implicitWidth
        implicitHeight: launcher.contentHeight
        visible: launcher.visible
        color: "transparent"
        focusable: false
        aboveWindows: true
        exclusiveZone: 0
        surfaceFormat.opaque: false
        mask: Region {}

        anchors.top: launcher.edge === "top" || launcher.edge === "left"
            || launcher.edge === "right"
        anchors.bottom: launcher.edge === "bottom"
        anchors.left: launcher.edge === "left" || launcher.edge === "top"
            || launcher.edge === "bottom"
        anchors.right: launcher.edge === "right"

        margins.top: launcher.edge === "top"
            ? Math.round(launcher.animatedMargin)
            : (launcher.edge === "left" || launcher.edge === "right"
                ? launcher.longAxisMargin() : 0)
        margins.bottom: launcher.edge === "bottom"
            ? Math.round(launcher.animatedMargin) : 0
        margins.left: launcher.edge === "left"
            ? Math.round(launcher.animatedMargin)
            : (launcher.edge === "top" || launcher.edge === "bottom"
                ? launcher.longAxisMargin() : 0)
        margins.right: launcher.edge === "right"
            ? Math.round(launcher.animatedMargin) : 0

        // Top keeps the material below the launcher's interactive Overlay
        // surface. The full-screen dismiss catcher is also transparent, so it
        // can remain above this surface and still pass the material through.
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "garage-launcher-glass"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    }

    // The search engine is resolved by garage and published as a
    // URL template, so the launcher does not have to know the engine list.
    FileView {
        id: engineFile
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/search-engine"
        printErrors: false
        watchChanges: true
    }

    // xdg-settings is the source of truth for the default browser; $BROWSER is
    // routinely stale. If the default is missing, fall back to any installed
    // browser rather than failing.
    Process {
        id: browserProbe
        running: true
        command: ["sh", "-c", `
            id=$(xdg-settings get default-web-browser 2>/dev/null)
            for dir in "$HOME/.local/share/applications" /usr/share/applications; do
                if [ -n "$id" ] && [ -f "$dir/$id" ]; then echo "$id"; exit 0; fi
            done
            grep -rl 'Categories=.*WebBrowser' \
                "$HOME/.local/share/applications" /usr/share/applications 2>/dev/null \
                | head -1 | xargs -r basename
        `]
        stdout: StdioCollector {
            onStreamFinished: {
                launcher.browserId = String(text || "").trim().replace(/\.desktop$/, "");
                launcher.browserResolved = true;
            }
        }
    }

    readonly property var browserEntry:
        browserId === "" ? null : DesktopEntries.byId(browserId)

    LauncherSources {
        id: extraSources
        onChanged: launcher.scheduleRebuild()
    }

    function searchUrl(text) {
        const template = String(engineFile.text() || "").trim()
            || "https://www.google.com/search?q=%s";
        return template.replace("%s", encodeURIComponent(text));
    }

    // Allocate every possible result slot once. The ListView keeps the same
    // model and delegates for the launcher's lifetime; filtering only rewrites
    // string roles in those slots, so a keystroke never clears or shortens the
    // model out from underneath the visible rows.
    function ensureResultSlots() {
        while (results.count < launcher.maxResultRows) {
            results.append({ kind: "", title: "", subtitle: "", icon: "" });
        }
    }

    function rebuild() {
        launcher.ensureResultSlots();
        const rows = [];
        const text = query.trim();
        const needle = text.toLowerCase();

        const extras = extraSources.rowsFor(text, NotificationDaemon.dnd,
            launcher.caffeine, Theme.dark, launcher.maxRows,
            launcher.clipboardMode);
        for (const row of extras.rows)
            rows.push(row);

        const sum = extras.exclusive ? null : Calculator.evaluate(text);
        if (sum !== null)
            rows.push({ kind: "calc", title: sum, subtitle: text + " — copy result",
                        icon: "", entry: null, url: "", value: sum });

        // Nothing until something is typed. An empty query used to rank every
        // installed application equally, so the launcher opened onto the first
        // eight of them in alphabetical order -- eight things the user had not
        // asked for, one of which was selected and one Return from launching.
        const apps = [];
        const model = needle === "" || extras.exclusive
            ? null : DesktopEntries.applications;
        for (let i = 0; model !== null && i < model.values.length; ++i) {
            const entry = model.values[i];
            const rank = LauncherExtras.applicationRank(entry, needle);
            if (rank >= 0)
                apps.push({ rank: rank, entry: entry });
        }
        apps.sort((a, b) => a.rank - b.rank
            || String(a.entry.name).localeCompare(String(b.entry.name)));
        const fileRows = extras.exclusive
            ? [] : extraSources.fileRowsFor(text, launcher.maxRows, false);
        const graphicalApps = apps.filter(app => !app.entry.runInTerminal);
        const terminalApps = apps.filter(app => app.entry.runInTerminal);
        const graphicalLimit = fileRows.length > 0
            ? Math.min(4, launcher.maxRows) : launcher.maxRows;
        function appendApp(app) {
            rows.push({ kind: "app", title: app.entry.name,
                        subtitle: app.entry.comment || app.entry.genericName || "",
                        icon: app.entry.icon, entry: app.entry, url: "" });
        }
        for (const app of graphicalApps.slice(0, graphicalLimit))
            appendApp(app);
        for (const row of fileRows.slice(0, 4))
            rows.push(row);
        const applicationRoom = Math.max(0, launcher.maxRows
            - Math.min(graphicalApps.length, graphicalLimit)
            - Math.min(fileRows.length, 4));
        for (const app of terminalApps.slice(0, applicationRoom))
            appendApp(app);

        // An address the user typed, and the site a bare name stands for. Both
        // go above the search row and below the applications: someone who types
        // "steam" with Steam installed wants the client, and someone who types
        // "github.com" wants the site rather than a search for its name.
        const address = extras.exclusive ? "" : WebAddress.addressFor(text);
        const site = extras.exclusive || address !== ""
            ? "" : WebAddress.siteFor(text);
        const destination = address !== "" ? address
            : (site !== "" ? "https://" + site : "");
        const browserName = browserResolved && browserId === ""
            ? "No web browser installed"
            : (browserEntry ? browserEntry.name : "Web browser");
        const openRow = destination === "" ? null
            : { kind: "url", title: "Open " + WebAddress.displayHost(destination),
                subtitle: browserName, icon: browserEntry ? browserEntry.icon : "",
                entry: null, url: destination };
        const searchRow = text === "" || extras.exclusive ? null
            : { kind: "web", title: "Search for “" + text + "”",
                subtitle: browserResolved && browserId === ""
                    ? "No web browser installed"
                    : (browserEntry ? browserEntry.name : "Web search"),
                icon: browserEntry ? browserEntry.icon : "", entry: null, url: "" };

        // Which of the two goes first, when both are offered. A named site or a
        // confident address is what the user meant; a bare word on an unusual
        // top-level domain parses as an address but is more often a filename,
        // so the search takes the default and opening it stays one row down.
        const openFirst = site !== "" || (address !== "" && WebAddress.confident(text));
        for (const row of (openFirst ? [openRow, searchRow] : [searchRow, openRow])) {
            if (row !== null)
                rows.push(row);
        }

        // What each row does, kept out of the model. `entry` is a DesktopEntry --
        // a QObject -- and a ListModel role holding one cannot be rewritten in
        // place: set() aborts the process outright when the role's type moves,
        // which it does the moment an application row is overwritten by a
        // search row. The model gets strings only; this array is what activate()
        // reads, and it matches the committed result count by construction.
        const displayedRows = rows.slice(0, launcher.maxResultRows);
        launcher.rowActions = displayedRows.map(row => ({
            kind: row.kind, entry: row.entry, url: row.url, title: row.title,
            value: row.value, action: row.action, command: row.command,
            pid: row.pid, target: row.target, currency: row.currency,
            path: row.path, durationMs: row.durationMs, label: row.label,
            timerId: row.timerId, clipId: row.clipId
        }));

        // Rewrite every preallocated slot in place. Slots beyond the current
        // result count receive the same four string roles with empty values;
        // their delegates remain alive but sit outside the committed viewport.
        for (let index = 0; index < launcher.maxResultRows; ++index) {
            const source = index < displayedRows.length ? displayedRows[index] : null;
            const row = { kind: String(source ? source.kind : ""),
                          title: String(source ? source.title || "" : ""),
                          subtitle: String(source ? source.subtitle || "" : ""),
                          icon: String(source ? source.icon || "" : "") };
            results.set(index, row);
        }
        // Give the ListView a real viewport and at least one polish turn before
        // publishing the new panel height. Without this staging, the layout can
        // grow around an empty result slot, move the field, then settle back as
        // its delegates appear a frame later.
        launcher.pendingRowCount = displayedRows.length;
        launcher.geometryWaits = 0;
        resultList.forceLayout();
        geometryCommit.restart();
        selected = 0;
    }

    ListModel { id: results }

    // Parallel to `results`, holding the parts of a row that are not text: the
    // desktop entry to launch and the address to open. See the note in
    // rebuild() for why these cannot live in the model.
    property var rowActions: []

    // Filtering an application catalog for every key event also makes pending
    // layouts overtake each other during quick typing. Keep the previous stable
    // panel until the query has paused briefly, then commit one complete result
    // set. The field itself remains immediate because its TextInput owns text.
    function scheduleRebuild() {
        geometryCommit.stop();
        launcher.pendingRowCount = launcher.rowCount;
        if (!launcher.clipboardMode)
            extraSources.prepareFiles(launcher.query);
        rebuildTimer.restart();
    }

    Timer {
        id: rebuildTimer
        interval: 55
        onTriggered: {
            // A failed helper still completes with an empty answer, so there is
            // no need for a timeout that briefly publishes app-only results.
            // Commit exactly once, after the matching file answer is present.
            if (extraSources.filePending(launcher.query))
                return;
            launcher.rebuild();
        }
    }

    // A zero-result commit needs no delegate. Otherwise wait until the final
    // visible row has an item, with a bounded fallback so an offscreen or failed
    // delegate can never hold the launcher open in a pending state forever.
    Timer {
        id: geometryCommit
        // One nominal 60 Hz frame: forceLayout gets a complete scene-polish
        // turn before the panel adopts the pending height.
        interval: 16
        onTriggered: {
            resultList.forceLayout();
            const last = launcher.pendingVisibleRows - 1;
            if (last >= 0 && resultList.itemAtIndex(last) === null
                    && launcher.geometryWaits < 8) {
                launcher.geometryWaits += 1;
                geometryCommit.restart();
                return;
            }
            launcher.rowCount = launcher.pendingRowCount;
        }
    }

    onQueryChanged: scheduleRebuild()
    onBrowserResolvedChanged: scheduleRebuild()
    onInitialModeChanged: {
        launcher.query = "";
        noBrowser.visible = false;
        launcher.scheduleRebuild();
        input.forceActiveFocus();
    }
    Component.onCompleted: {
        if (!launcher.clipboardMode)
            extraSources.prepareFiles(query);
        rebuild();
        // The compositor hands the layer surface the keyboard as it maps; this
        // is what points it at the search field rather than at the window, so
        // the first keystroke after the bind is typed into the query.
        input.forceActiveFocus();
    }

    function activate(index) {
        if (index < 0 || index >= launcher.rowCount
                || index >= launcher.rowActions.length)
            return;
        const row = launcher.rowActions[index];
        if (row.kind === "app") {
            row.entry.execute();
        } else if (["calc", "unit", "currency", "emoji", "uuid", "random"].includes(row.kind)) {
            Quickshell.clipboardText = String(row.value || row.title);
        } else if (row.kind.indexOf("session-") === 0) {
            launcher.sessionActionRequested(String(row.action));
            return;
        } else if (row.kind.indexOf("shell-") === 0) {
            launcher.shellActionRequested(String(row.action));
            return;
        } else if (row.kind.indexOf("media-") === 0) {
            if (row.command)
                Quickshell.execDetached(row.command);
            else
                MediaController.dispatch(String(row.action));
        } else if (row.kind.indexOf("clock-") === 0) {
            TimerService.activate(row);
        } else if (row.kind === "file" || row.kind === "directory") {
            Quickshell.execDetached(["xdg-open", row.path]);
        } else if (row.kind === "process") {
            Quickshell.execDetached(["kill", "-TERM", String(row.pid)]);
        } else if (row.kind === "ssh") {
            // uwsm -T resolves the terminal published by System Preferences;
            // the validated target remains one argv value all the way to ssh.
            Quickshell.execDetached(["uwsm", "app", "-T", "ssh", "--", row.target]);
        } else if (row.kind === "clip") {
            const clipId = String(row.clipId || "");
            if (!/^[0-9]+$/.test(clipId))
                return;
            // The database id is the only interpolated value and is passed as
            // a positional argument after numeric validation. The preview is
            // never decoded by the shell. Keeping the pipe preserves images
            // and arbitrary bytes instead of coercing everything through a
            // QML string clipboard property.
            Quickshell.execDetached(["sh", "-c",
                "cliphist decode \"$1\" | wl-copy",
                "garage-clipboard", clipId]);
        } else if (row.kind === "currency-error") {
            extraSources.retryCurrency(row.currency);
            return;
        } else if (row.kind === "status" || row.kind === "error") {
            return;
        } else if (row.kind === "web" || row.kind === "url") {
            if (browserId === "") {
                noBrowser.visible = true;
                return;
            }
            Quickshell.execDetached(["xdg-open", row.kind === "url"
                ? row.url : searchUrl(query.trim())]);
        }
        launcher.requestDismissal();
    }

    ContinuousRectangle {
        id: panel
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: launcher.edge === "bottom" ? undefined : parent.top
        anchors.bottom: launcher.edge === "bottom" ? parent.bottom : undefined
        height: launcher.contentHeight
        clip: true
        opacity: launcher.contentOpacity
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass: glassSurface is directly beneath this host,
        // so the material shows through and painting a body here would cover it.
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
        // every other panel in the shell draws. insetRadius keeps the two
        // concentric at every corner radius setting.
        ContinuousRectangle {
            anchors.fill: parent
            anchors.margins: 1
            radius: Theme.insetRadius(panel.radius, 1)
            power: Theme.cornerPower
            borderWidth: 1
            borderColor: Theme.frameInner
        }

        // The vertical axis is explicit. A ColumnLayout is allowed to distribute
        // surplus or constrained height among its children while a ListView is
        // populating; that was the transient downward push of the field. Only
        // the field's fixed height and the committed row count participate here.
        Item {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: launcher.contentMargin
            implicitHeight: launcher.fieldHeight + (launcher.listing
                ? launcher.listGap + launcher.visibleRows * launcher.rowHeight : 0)
            height: implicitHeight

            RowLayout {
                id: fieldRow
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: launcher.fieldHeight
                spacing: 12

                Item {
                    Layout.preferredWidth: 22
                    Layout.preferredHeight: 22
                    Image {
                        id: searchGlyph
                        anchors.fill: parent
                        source: "icons/magnifying-glass.svg"
                        sourceSize.width: 44
                        sourceSize.height: 44
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        mipmap: true
                        visible: false
                    }
                    ColorOverlay {
                        anchors.fill: searchGlyph
                        source: searchGlyph
                        color: Theme.textMuted
                        cached: true
                    }
                }

                TextInput {
                    id: input
                    Layout.fillWidth: true
                    Layout.preferredHeight: launcher.fieldHeight
                    text: launcher.query
                    onTextChanged: launcher.query = text
                    color: Theme.text
                    selectionColor: Theme.accent
                    selectedTextColor: Theme.accentText
                    font.family: Theme.sans
                    font.pixelSize: 20
                    verticalAlignment: Text.AlignVCenter
                    selectByMouse: true
                    clip: true

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: launcher.clipboardMode
                            ? "Search clipboard history"
                            : "Search apps, commands, conversions, or the web"
                        color: Theme.textDisabled
                        font: input.font
                        visible: input.text === ""
                        renderType: Text.NativeRendering
                    }

                    // Clamped at both ends: with nothing in the list at all,
                    // count - 1 is -1, and Down landing on -1 leaves the first
                    // row unselectable until something else resets it.
                    Keys.onDownPressed: launcher.selected = Math.max(0,
                        Math.min(launcher.selected + 1, launcher.rowCount - 1))
                    Keys.onUpPressed: launcher.selected =
                        Math.max(launcher.selected - 1, 0)
                    Keys.onReturnPressed: launcher.activate(launcher.selected)
                    Keys.onEnterPressed: launcher.activate(launcher.selected)
                }
            }

            ListView {
                id: resultList
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: fieldRow.bottom
                anchors.topMargin: launcher.listGap
                height: launcher.renderRows * launcher.rowHeight
                // A pending list is real and laid out, but not exposed until its
                // final visible delegate exists and geometryCommit publishes the
                // matching panel height.
                visible: launcher.renderRows > 0
                opacity: launcher.listing ? 1 : 0
                model: results
                clip: true
                currentIndex: launcher.selected
                boundsBehavior: Flickable.StopAtBounds

                delegate: LauncherResult {
                    required property int index
                    required property var model
                    width: resultList.width
                    height: launcher.rowHeight
                    kind: model.kind
                    title: model.title
                    subtitle: model.subtitle
                    iconName: model.icon
                    selected: index === launcher.selected
                    onHovered: launcher.selected = index
                    onActivated: launcher.activate(index)
                }
            }
        }
    }

    // Shown instead of failing silently when a web search has nowhere to go.
    Rectangle {
        id: noBrowser
        anchors.fill: panel
        visible: false
        color: Theme.scrim

        ContinuousRectangle {
            anchors.centerIn: parent
            width: 380
            height: 150
            color: Theme.surfaceRaised
            borderWidth: 1
            borderColor: Theme.border

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 20
                spacing: 10

                Text {
                    Layout.fillWidth: true
                    text: "No web browser installed"
                    color: Theme.text
                    font.family: Theme.sans
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    renderType: Text.NativeRendering
                }

                Text {
                    Layout.fillWidth: true
                    text: "Install a browser to search the web from the launcher."
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 12
                    wrapMode: Text.WordWrap
                    renderType: Text.NativeRendering
                }

                Item { Layout.fillHeight: true }

                SettingsButton {
                    Layout.alignment: Qt.AlignRight
                    text: "OK"
                    prominent: true
                    onClicked: noBrowser.visible = false
                }
            }
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: {
            if (noBrowser.visible)
                noBrowser.visible = false;
            else
                launcher.requestDismissal();
        }
    }
}
