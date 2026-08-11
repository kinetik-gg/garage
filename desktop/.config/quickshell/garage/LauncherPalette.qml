import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

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

    readonly property real rowHeight: 52
    readonly property int maxRows: 8

    // Where the launcher opens, every time. Both of these are held here rather
    // than left to the compositor, which centres an unanchored layer surface on
    // its output: centred, a surface whose height tracks the result count moved
    // the query field up the screen with every keystroke that changed how many
    // results there were. Anchored to the top, the field stays where it opened
    // and the list grows downward under it.
    // A third of the way down the output, measured from the top of the usable
    // area rather than from the top of the screen -- an overlay surface already
    // begins below Waybar's exclusive zone, so a margin here is measured from
    // there. The field is what sits at this height: the list grows downward
    // underneath it and never moves it.
    readonly property real spawnTop: {
        const target = launcher.targetScreen();
        const available = target ? target.height : 1080;
        return Math.max(Theme.windowGutter, Math.round(available / 3));
    }
    readonly property real spawnLeft: {
        const target = launcher.targetScreen();
        const available = target ? target.width : 1920;
        return Math.max(Theme.windowGutter,
                        (available - launcher.implicitWidth) / 2);
    }

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
    readonly property real contentMargin: 14

    // The row count the window is sized from. Deliberately not results.count.
    //
    // rebuild() fills the model a row at a time, and every one of those
    // append() and remove() calls moves results.count. Sized from it directly,
    // narrowing a nine-row list to two resized the layer surface seven times
    // for one keystroke -- each resize a configure, a new buffer and a relayout
    // of everything in the panel. That is what moves around while typing.
    //
    // Set once, at the end of a rebuild, so the window changes size exactly
    // once no matter how many rows came or went.
    property int rowCount: 0
    readonly property bool listing: launcher.rowCount > 0
    readonly property int visibleRows: Math.min(launcher.rowCount, launcher.maxRows)

    // Measured off the column rather than recomputed from its parts, the way
    // every other palette in the shell sizes itself. Arithmetic written out here
    // -- margins plus field plus gap plus rows -- has to be kept in step with
    // the layout by hand, and the two disagreeing is a panel whose contents hang
    // off the bottom of the surface they are drawn on.
    //
    // Exactly the contents and not a pixel more, on every frame. The compositor
    // blurs a surface rather than what is painted on it, so any height the panel
    // is not using comes out as a blurred slab hanging below the last row.
    implicitHeight: Math.max(1, body.implicitHeight + launcher.contentMargin * 2)
    color: "transparent"
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    // Top-to-bottom entrance, shared with every other palette. See PanelMotion.
    PanelMotion {
        id: motion
        restingTop: launcher.spawnTop
        onFinished: launcher.dismissed()
    }

    function requestDismissal() {
        motion.dismiss();
    }

    anchors {
        top: true
        left: true
    }

    // Clamped into the output on both axes, so a drag cannot put the field
    // somewhere it can be typed into but not seen.
    margins.left: launcher.spawnLeft
    margins.top: motion.surfaceTop

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "garage-launcher"
    // OnDemand, not Exclusive. An exclusive layer keyboard is held at the
    // protocol level no matter where the pointer goes, which takes every
    // keystroke in the session for as long as the launcher is up. On demand is
    // enough: the compositor hands this surface the keyboard as it maps, which
    // is what makes the query field typeable the moment the bind is pressed and
    // Escape heard without a click first. Leave this alone -- typing here works
    // today, and Exclusive is what it was before.
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
                        icon: "", entry: null, url: "" });

        // Nothing until something is typed. An empty query used to rank every
        // installed application equally, so the launcher opened onto the first
        // eight of them in alphabetical order -- eight things the user had not
        // asked for, one of which was selected and one Return from launching.
        const apps = [];
        const model = needle === "" ? null : DesktopEntries.applications;
        for (let i = 0; model !== null && i < model.values.length; ++i) {
            const entry = model.values[i];
            const rank = matches(entry, needle);
            if (rank >= 0)
                apps.push({ rank: rank, entry: entry });
        }
        apps.sort((a, b) => a.rank - b.rank
            || String(a.entry.name).localeCompare(String(b.entry.name)));
        for (const app of apps.slice(0, maxRows))
            rows.push({ kind: "app", title: app.entry.name,
                        subtitle: app.entry.comment || app.entry.genericName || "",
                        icon: app.entry.icon, entry: app.entry, url: "" });

        // An address the user typed, and the site a bare name stands for. Both
        // go above the search row and below the applications: someone who types
        // "steam" with Steam installed wants the client, and someone who types
        // "github.com" wants the site rather than a search for its name.
        const address = WebAddress.addressFor(text);
        const site = address === "" ? WebAddress.siteFor(text) : "";
        const destination = address !== "" ? address
            : (site !== "" ? "https://" + site : "");
        const browserName = browserResolved && browserId === ""
            ? "No web browser installed"
            : (browserEntry ? browserEntry.name : "Web browser");
        const openRow = destination === "" ? null
            : { kind: "url", title: "Open " + WebAddress.displayHost(destination),
                subtitle: browserName, icon: browserEntry ? browserEntry.icon : "",
                entry: null, url: destination };
        const searchRow = text === "" ? null
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
        // reads, and the two are the same length by construction.
        launcher.rowActions = rows.map(row => ({ kind: row.kind, entry: row.entry,
                                                 url: row.url, title: row.title }));

        // Updated in place, never cleared. clear() destroys every delegate the
        // ListView is holding and append() builds them all again, so a keystroke
        // that changed one row tore down the whole list and rebuilt it -- and
        // for the frame in between the list was empty. That was the flicker: the
        // results blinking out on every character, not the window resizing
        // around them. set() rewrites a row the ListView already has, so its
        // delegate survives and repaints.
        for (let index = 0; index < rows.length; ++index) {
            const source = rows[index];
            const row = { kind: String(source.kind),
                          title: String(source.title || ""),
                          subtitle: String(source.subtitle || ""),
                          icon: String(source.icon || "") };
            if (index < results.count)
                results.set(index, row);
            else
                results.append(row);
        }
        while (results.count > rows.length)
            results.remove(results.count - 1);
        launcher.rowCount = rows.length;
        selected = 0;
    }

    ListModel { id: results }

    // Parallel to `results`, holding the parts of a row that are not text: the
    // desktop entry to launch and the address to open. See the note in
    // rebuild() for why these cannot live in the model.
    property var rowActions: []

    onQueryChanged: rebuild()
    onBrowserResolvedChanged: rebuild()
    Component.onCompleted: {
        rebuild();
        // The compositor hands the layer surface the keyboard as it maps; this
        // is what points it at the search field rather than at the window, so
        // the first keystroke after the bind is typed into the query.
        input.forceActiveFocus();
    }

    function activate(index) {
        if (index < 0 || index >= results.count
                || index >= launcher.rowActions.length)
            return;
        const row = launcher.rowActions[index];
        if (row.kind === "app") {
            row.entry.execute();
        } else if (row.kind === "calc") {
            Quickshell.clipboardText = row.title;
        } else if (row.kind === "web" || row.kind === "url") {
            if (browserId === "") {
                noBrowser.visible = true;
                return;
            }
            Quickshell.execDetached(["xdg-open", row.kind === "url"
                ? row.url : searchUrl(query.trim())]);
        }
        launcher.dismissed();
    }

    ContinuousRectangle {
        id: panel
        anchors.fill: parent
        opacity: motion.opacity
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass: garage-launcher is one of the compositor's
        // glass layer namespaces, so the material is drawn beneath this surface
        // and painting a body here would cover it.
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

        // Anchored rather than filled: the window's height is this column's
        // implicit height, so the column must not be told how tall it is. Filled,
        // it took the surface's height instead -- and whenever the surface was
        // momentarily taller than the contents, the layout shared the surplus out
        // between the field and the list and both drifted toward the middle of
        // the panel until the sizes agreed again.
        ColumnLayout {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: launcher.contentMargin
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

                    // Clamped at both ends: with nothing in the list at all,
                    // count - 1 is -1, and Down landing on -1 leaves the first
                    // row unselectable until something else resets it.
                    Keys.onDownPressed: launcher.selected = Math.max(0,
                        Math.min(launcher.selected + 1, results.count - 1))
                    Keys.onUpPressed: launcher.selected =
                        Math.max(launcher.selected - 1, 0)
                    Keys.onReturnPressed: launcher.activate(launcher.selected)
                    Keys.onEnterPressed: launcher.activate(launcher.selected)
                }
            }

            ListView {
                id: resultList
                Layout.fillWidth: true
                // Sized rather than filling: the window's height is computed
                // from this above, so a fillHeight here would be each asking
                // the other how tall it is.
                Layout.preferredHeight: launcher.visibleRows * launcher.rowHeight
                visible: launcher.listing
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
