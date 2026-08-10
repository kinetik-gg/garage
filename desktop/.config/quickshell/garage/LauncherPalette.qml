import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

// A layer surface, like every other transient palette in the shell, and unlike
// the FloatingWindow this used to be. A toplevel is dismissed by losing focus,
// and under focus-follows-mouse merely moving the pointer across another window
// took that focus away -- so the launcher closed on a cursor twitch. A layer
// surface has no such notion: it is dismissed by the focus grab below (a click
// outside), by Escape, or by the keybind toggling the loader, and by nothing
// else.
PanelWindow {
    id: launcher

    signal dismissed()

    // The monitor the launcher was asked for on, resolved by the shell at the
    // moment of the keypress and held as a name. Deliberately not read live from
    // Hyprland's focused monitor: under focus-follows-mouse that changes as soon
    // as the pointer crosses a screen edge, and an open launcher that jumps to
    // another monitor when the cursor drifts is the bug this file is fixing
    // wearing a different hat.
    required property string targetScreenName

    property string query: ""
    property int selected: 0
    // Desktop id of the browser to hand web searches to. Empty means none was
    // found, which the launcher has to say rather than silently doing nothing.
    property string browserId: ""
    property bool browserResolved: false
    // Armed a turn after the surface exists: the grab is cleared as it is taken
    // if it is armed in the same turn as the click that opened the launcher.
    property bool grabReady: false

    readonly property real rowHeight: 52
    readonly property int maxRows: 8

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === launcher.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    screen: launcher.targetScreen()
    implicitWidth: 640
    implicitHeight: Math.min(
        72 + Math.max(results.count, 1) * rowHeight + 12,
        72 + maxRows * rowHeight + 12)
    color: "transparent"
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    // Deliberately no anchors: a layer surface anchored to nothing is centred on
    // its output by the compositor, which is where a launcher belongs. The
    // implicit size below is the whole geometry, so the height still tracks the
    // result count as it did when this was a window.

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "garage-launcher"
    // Exclusive rather than OnDemand: this is a typing surface, and the search
    // field has to have the keyboard the moment it appears rather than after a
    // click. The grab below is the dismissal gesture, not the way in.
    // OnDemand, not Exclusive: an exclusive layer keyboard is held at the
    // protocol level no matter where the pointer clicks, so the focus grab
    // below would never clear and a click outside could never dismiss. With
    // OnDemand the grab is what delivers the keyboard on open -- and what
    // hands it back, dismissing us, when a click lands anywhere else.
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

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

    function searchUrl(text) {
        const template = String(engineFile.text() || "").trim()
            || "https://www.google.com/search?q=%s";
        return template.replace("%s", encodeURIComponent(text));
    }

    function matches(entry, needle) {
        if (entry.noDisplay)
            return -1;
        const name = String(entry.name || "").toLowerCase();
        if (name.startsWith(needle)) return 0;
        if (name.includes(needle)) return 1;
        if (String(entry.genericName || "").toLowerCase().includes(needle)) return 2;
        if (String(entry.comment || "").toLowerCase().includes(needle)) return 3;
        return -1;
    }

    function rebuild() {
        const rows = [];
        const text = query.trim();
        const needle = text.toLowerCase();

        const sum = Calculator.evaluate(text);
        if (sum !== null)
            rows.push({ kind: "calc", title: sum, subtitle: text + " — copy result",
                        icon: "", entry: null });

        const apps = [];
        const model = DesktopEntries.applications;
        for (let i = 0; i < model.values.length; ++i) {
            const entry = model.values[i];
            const rank = needle === "" ? (entry.noDisplay ? -1 : 1) : matches(entry, needle);
            if (rank >= 0)
                apps.push({ rank: rank, entry: entry });
        }
        apps.sort((a, b) => a.rank - b.rank
            || String(a.entry.name).localeCompare(String(b.entry.name)));
        for (const app of apps.slice(0, maxRows))
            rows.push({ kind: "app", title: app.entry.name,
                        subtitle: app.entry.comment || app.entry.genericName || "",
                        icon: app.entry.icon, entry: app.entry });

        if (text !== "")
            rows.push({ kind: "web", title: "Search for “" + text + "”",
                        subtitle: browserResolved && browserId === ""
                            ? "No web browser installed"
                            : (browserEntry ? browserEntry.name : "Web search"),
                        icon: browserEntry ? browserEntry.icon : "", entry: null });

        results.clear();
        for (const row of rows)
            results.append(row);
        selected = 0;
    }

    ListModel { id: results }

    onQueryChanged: rebuild()
    onBrowserResolvedChanged: rebuild()
    Component.onCompleted: {
        rebuild();
        // The compositor hands an exclusive layer surface the keyboard as it
        // maps; this is what points it at the search field rather than at the
        // window, so the first keystroke after the bind is typed into the query.
        input.forceActiveFocus();
        Qt.callLater(() => launcher.grabReady = true);
    }

    function activate(index) {
        if (index < 0 || index >= results.count)
            return;
        const row = results.get(index);
        if (row.kind === "app") {
            row.entry.execute();
        } else if (row.kind === "calc") {
            Quickshell.clipboardText = row.title;
        } else if (row.kind === "web") {
            if (browserId === "") {
                noBrowser.visible = true;
                return;
            }
            Quickshell.execDetached(["xdg-open", searchUrl(query.trim())]);
        }
        launcher.dismissed();
    }

    ContinuousRectangle {
        anchors.fill: parent
        color: Theme.dialogTint

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 14
            spacing: 8

            RowLayout {
                Layout.fillWidth: true
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
                    Layout.preferredHeight: 34
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
                        text: "Search apps, calculate, or search the web"
                        color: Theme.textDisabled
                        font: input.font
                        visible: input.text === ""
                        renderType: Text.NativeRendering
                    }

                    Keys.onDownPressed: launcher.selected =
                        Math.min(launcher.selected + 1, results.count - 1)
                    Keys.onUpPressed: launcher.selected =
                        Math.max(launcher.selected - 1, 0)
                    Keys.onReturnPressed: launcher.activate(launcher.selected)
                    Keys.onEnterPressed: launcher.activate(launcher.selected)
                }
            }

            ListView {
                id: resultList
                Layout.fillWidth: true
                Layout.fillHeight: true
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
        anchors.fill: parent
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

    // Clicking anywhere outside the launcher dismisses it. This is the whole of
    // the click-outside gesture: moving the pointer over another window does not
    // clear a grab, which is exactly why the launcher is a layer surface now.
    HyprlandFocusGrab {
        active: launcher.grabReady
        windows: [launcher]
        onCleared: {
            if (launcher.grabReady)
                launcher.dismissed();
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: {
            if (noBrowser.visible)
                noBrowser.visible = false;
            else
                launcher.dismissed();
        }
    }
}
