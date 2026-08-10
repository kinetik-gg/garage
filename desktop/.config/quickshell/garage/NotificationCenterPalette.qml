import Quickshell
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

// The notification centre: everything the daemon is still holding, drawn as a
// column down the right edge of one screen.
//
// A layer surface rather than a FloatingWindow, unlike Preferences and About. It
// is a panel pinned to an edge that the next click dismisses, not a window the
// user arranges, and it has to sit above fullscreen clients to be reachable at
// all -- both of which are what the overlay layer is for.
//
// The centre owns the exit animations for everything it closes. A notification
// leaves the daemon the moment it is dismissed, so a card cannot play itself out
// after the fact: the wave below plays the cards first and dismisses afterwards,
// which is the same order the toast stack uses.
PanelWindow {
    id: centre

    required property string targetScreenName

    signal dismissed()

    // The list, grouped by app. A plain property fed by refresh() rather than a
    // binding on the model: the list is deliberately *not* rebuilt while cards
    // are playing out or while a reply is being typed, and a binding cannot
    // decline to re-evaluate. Rebuilding it recreates every delegate, which
    // would restart mid-flight animations and take the keyboard out of a reply
    // field halfway through a sentence.
    property var groups: []
    property bool refreshPending: false

    // The closing wave. `pending` is queued but still drawn normally, `closing`
    // is playing out, and `dropped` is what the wave has finished with -- kept
    // out of the list by hand until the model catches up with the dismissals,
    // because a dismissed notification is not gone from the model until the last
    // RetainableLock on it is released.
    property var pending: []
    property var closing: []
    property var dropped: []
    property real waveStartedAt: 0
    property real waveEndsAt: 0

    // The notification whose reply field currently has the keyboard. Held so the
    // list can refuse to rebuild under it; nothing else reads it.
    property var replyTarget: null

    readonly property bool frozen: centre.closing.length > 0
        || centre.pending.length > 0 || centre.replyTarget !== null

    // Wave timings. The stagger is what makes clearing read as an action rather
    // than a glitch; the budget caps how long the whole wave may take, because
    // the history holds a hundred rows and the ones below the fold are not on
    // screen to be staggered anyway. The grace is a frame past NotificationCard's
    // own 150 ms exit, so a card finishes what it started even when nothing
    // reports that it did.
    readonly property int stagger: 32
    readonly property int staggerBudget: 320
    readonly property int exitGrace: 170

    readonly property int contentMargin: 12

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === centre.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    // Same fallback chain as NotificationCard's own icon, for the group header.
    // Repeated rather than shared through the daemon: the daemon is the bus
    // service, and which icon a header draws is not its business.
    function appIconFor(notification) {
        if (!notification)
            return "";
        const icon = String(notification.appIcon || "");
        if (icon === "")
            return "";
        if (icon.startsWith("/") || icon.startsWith("file:") || icon.startsWith("image:"))
            return icon;
        return Quickshell.iconPath(icon, true);
    }

    // Group by app name, newest group first, newest member first inside a group.
    // By name rather than by desktop entry: a sender that publishes no entry --
    // a script sending through notify-send, a browser page -- would otherwise all
    // land in one nameless group together.
    function buildGroups(live) {
        const order = [];
        const byName = ({});

        for (const notification of live) {
            if (centre.dropped.indexOf(notification) !== -1)
                continue;

            const name = String(notification.appName || "").trim() || "Notification";
            let group = byName[name];
            if (group === undefined) {
                group = { name: name, newest: 0, iconSource: "", notifications: [] };
                byName[name] = group;
                order.push(group);
            }

            group.notifications.push(notification);
            const arrival = NotificationDaemon.arrivalTime(notification);
            if (arrival >= group.newest) {
                group.newest = arrival;
                // The newest member's icon, so an app that changed it mid-session
                // is shown as it is sending now.
                group.iconSource = centre.appIconFor(notification);
            }
        }

        for (const group of order) {
            group.notifications.sort((first, second) =>
                NotificationDaemon.arrivalTime(second) - NotificationDaemon.arrivalTime(first));
        }

        order.sort((first, second) => second.newest - first.newest);
        return order;
    }

    function rebuild() {
        const live = NotificationDaemon.notifications.values;
        // Forget the rows the model has caught up with; anything still in it is
        // still being held by a lock somewhere and still has to be filtered.
        centre.dropped = centre.dropped.filter(
            notification => live.indexOf(notification) !== -1);
        centre.groups = centre.buildGroups(live);
        centre.refreshPending = false;
    }

    function refresh() {
        if (centre.frozen) {
            centre.refreshPending = true;
            return;
        }
        centre.rebuild();
    }

    // Every notification in the order it is drawn, which is the order a wave
    // plays out in.
    function everything() {
        const ordered = [];
        for (const group of centre.groups) {
            for (const notification of group.notifications)
                ordered.push(notification);
        }
        return ordered;
    }

    function isClosing(notification) {
        return notification !== null && notification !== undefined
            && centre.closing.indexOf(notification) !== -1;
    }

    // Play these cards out, then close them. Everything the centre closes goes
    // through here: one card's x, a group's x, and Clear All differ only in what
    // they hand over.
    function beginClose(notifications) {
        const queued = [];
        for (const notification of notifications) {
            if (!notification)
                continue;
            if (centre.closing.indexOf(notification) !== -1
                || centre.pending.indexOf(notification) !== -1)
                continue;
            queued.push(notification);
        }
        if (queued.length === 0)
            return;

        // A field that is about to be animated away does not keep the list
        // frozen, and the reply it was holding is not going to be sent.
        if (centre.replyTarget !== null && queued.indexOf(centre.replyTarget) !== -1)
            centre.replyTarget = null;

        if (centre.pending.length === 0 && centre.closing.length === 0)
            centre.waveStartedAt = Date.now();
        centre.pending = centre.pending.concat(queued);
        // Started here rather than on the next tick, so the first card moves on
        // the click that asked for it.
        centre.stepWave();
    }

    function stepWave() {
        if (centre.pending.length > 0) {
            const take = Date.now() - centre.waveStartedAt > centre.staggerBudget
                ? centre.pending.length : 1;
            centre.closing = centre.closing.concat(centre.pending.slice(0, take));
            centre.pending = centre.pending.slice(take);
            centre.waveEndsAt = Date.now() + centre.exitGrace;
            return;
        }

        if (Date.now() >= centre.waveEndsAt)
            centre.finishWave();
    }

    function finishWave() {
        const gone = centre.closing.slice();
        const live = NotificationDaemon.notifications.values;
        centre.closing = [];
        if (gone.length === 0) {
            centre.rebuild();
            return;
        }

        // Out of the list before anything is closed -- the same order the toast
        // stack finishes in. The row goes first, which destroys the card and the
        // retain lock it holds, so the notification is not closed underneath a
        // card that is still bound to it.
        centre.dropped = centre.dropped.concat(gone);
        centre.rebuild();

        if (gone.length === live.length) {
            // The whole history: that is the daemon's own call, and it is the
            // daemon that owns closing everything.
            NotificationDaemon.clearAll();
            return;
        }

        for (const notification of gone) {
            // A notification the sender withdrew while its card played out is
            // already gone, and a destroyed one reads as absent here rather than
            // throwing when it is asked to dismiss itself.
            if (live.indexOf(notification) !== -1)
                notification.dismiss();
        }
    }

    function clearAll() {
        centre.beginClose(centre.everything());
    }

    screen: centre.targetScreen()
    implicitWidth: 390
    color: "transparent"
    // OnDemand rather than Exclusive: the centre has a reply field to type into,
    // but it is a panel the user clicks into rather than a modal, and an
    // exclusive surface takes every keystroke in the session for as long as it
    // is up. The compositor hands an on-demand layer surface the keyboard as it
    // maps, which is what makes the Escape at the foot of this file heard
    // without a click first.
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
    WlrLayershell.namespace: "garage-notification-center"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    Component.onCompleted: centre.rebuild()

    onFrozenChanged: {
        if (!centre.frozen && centre.refreshPending)
            centre.rebuild();
    }

    // A wave the user walked out on still happened. Escape or a click outside
    // destroys the centre and every card in it, but Clear All was an instruction
    // to clear, not to start an animation.
    Component.onDestruction: {
        const live = NotificationDaemon.notifications.values;
        for (const notification of centre.closing.concat(centre.pending)) {
            if (live.indexOf(notification) !== -1)
                notification.dismiss();
        }
    }

    Connections {
        target: NotificationDaemon.notifications
        function onValuesChanged() {
            centre.refresh();
        }
    }

    // One tick for the whole wave: it starts the staggered cards and then reaps
    // the wave when the last of them is due to have finished. Off whenever
    // nothing is closing, so an open centre costs no wakeups.
    Timer {
        interval: centre.stagger
        repeat: true
        running: centre.closing.length > 0 || centre.pending.length > 0
        onTriggered: centre.stepWave()
    }

    ContinuousRectangle {
        id: panel
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass: garage-notification-center is one of the
        // compositor's glass layer namespaces, so the material is drawn beneath
        // this surface and painting a body here would cover it.
        color: Theme.panel
        borderWidth: 1
        borderColor: Theme.frameOuter

        // Whole-panel appear. One animation rather than a fade on every card:
        // the list is rebuilt whenever the history changes, and a per-card fade
        // would flash the entire column every time a notification arrived.
        NumberAnimation on opacity {
            from: 0
            to: 1
            duration: 130
            easing.type: Easing.OutCubic
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

        // The panel eats the clicks that land in the gaps between its cards
        // rather than leaving them unhandled.
        MouseArea {
            anchors.fill: parent
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: centre.contentMargin
            spacing: 10

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                Text {
                    text: "Notifications"
                    color: Theme.text
                    font.family: Theme.sans
                    font.pixelSize: 17
                    font.weight: Font.DemiBold
                    renderType: Text.NativeRendering
                }

                Item { Layout.fillWidth: true }

                // The moon is the switch's label: a bare switch in a header says
                // nothing about what it silences.
                Item {
                    Layout.preferredWidth: 14
                    Layout.preferredHeight: 14
                    Layout.alignment: Qt.AlignVCenter

                    Image {
                        id: moonGlyph
                        anchors.fill: parent
                        source: "icons/moon.svg"
                        sourceSize.width: 28
                        sourceSize.height: 28
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        antialiasing: true
                        mipmap: true
                        visible: false
                    }

                    ColorOverlay {
                        anchors.fill: moonGlyph
                        source: moonGlyph
                        color: NotificationDaemon.dnd ? Theme.text : Theme.textMuted
                        cached: true
                    }
                }

                // Wrapped, the way SettingsRow wraps its control: the switch
                // sizes itself with width and height rather than implicit ones,
                // and a layout that is free to write those would flatten it.
                Item {
                    Layout.preferredWidth: dndSwitch.width
                    Layout.preferredHeight: dndSwitch.height
                    Layout.alignment: Qt.AlignVCenter

                    SettingsSwitch {
                        id: dndSwitch
                        checked: NotificationDaemon.dnd
                        // setDnd rather than the alias: do-not-disturb is
                        // persisted, and every write has to reach the file.
                        onToggled: value => NotificationDaemon.setDnd(value)
                    }
                }

                SettingsButton {
                    Layout.alignment: Qt.AlignVCenter
                    text: "Clear All"
                    ghost: true
                    verticalPadding: 5
                    horizontalPadding: 10
                    enabled: centre.groups.length > 0
                    onClicked: centre.clearAll()
                }
            }

            MenuSeparator {}

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                ListView {
                    id: list
                    anchors.fill: parent
                    visible: centre.groups.length > 0
                    model: centre.groups
                    clip: true
                    spacing: 14
                    boundsBehavior: Flickable.StopAtBounds

                    delegate: Column {
                        id: groupBlock
                        required property var modelData

                        width: list.width
                        spacing: 6

                        Item {
                            width: groupBlock.width
                            height: 20

                            Item {
                                id: groupIcon
                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                width: 16
                                height: 16

                                readonly property string source:
                                    String(groupBlock.modelData.iconSource || "")
                                readonly property bool usesGlyph: groupIcon.source === ""

                                // Theme icons arrive already coloured and must
                                // not be overlaid.
                                Image {
                                    anchors.fill: parent
                                    visible: !groupIcon.usesGlyph
                                    source: groupIcon.usesGlyph ? "" : groupIcon.source
                                    sourceSize.width: 32
                                    sourceSize.height: 32
                                    fillMode: Image.PreserveAspectFit
                                    smooth: true
                                    mipmap: true
                                }

                                // The built-in glyph ships its own colour, so it
                                // goes through an overlay to follow the theme.
                                Image {
                                    id: groupBell
                                    anchors.fill: parent
                                    visible: false
                                    source: groupIcon.usesGlyph ? "icons/bell.svg" : ""
                                    sourceSize.width: 32
                                    sourceSize.height: 32
                                    fillMode: Image.PreserveAspectFit
                                    smooth: true
                                    antialiasing: true
                                    mipmap: true
                                }

                                ColorOverlay {
                                    anchors.fill: groupBell
                                    source: groupBell
                                    visible: groupIcon.usesGlyph
                                    color: Theme.textMuted
                                    cached: true
                                }
                            }

                            Text {
                                anchors.left: groupIcon.right
                                anchors.leftMargin: 7
                                anchors.right: groupClear.left
                                anchors.rightMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                text: String(groupBlock.modelData.name || "")
                                color: Theme.text
                                font.family: Theme.sans
                                font.pixelSize: 12
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                                renderType: Text.NativeRendering
                            }

                            ContinuousRectangle {
                                id: groupClear
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                implicitWidth: 20
                                implicitHeight: 20
                                radius: Theme.controlRadius
                                color: groupClearPointer.containsMouse
                                    ? Theme.hoverStrong : "transparent"

                                Image {
                                    id: groupClearGlyph
                                    anchors.centerIn: parent
                                    width: 11
                                    height: 11
                                    source: "icons/x.svg"
                                    sourceSize.width: 22
                                    sourceSize.height: 22
                                    fillMode: Image.PreserveAspectFit
                                    smooth: true
                                    antialiasing: true
                                    mipmap: true
                                    visible: false
                                }

                                ColorOverlay {
                                    anchors.fill: groupClearGlyph
                                    source: groupClearGlyph
                                    color: groupClearPointer.containsMouse
                                        ? Theme.text : Theme.textMuted
                                    cached: true
                                }

                                MouseArea {
                                    id: groupClearPointer
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: centre.beginClose(
                                        groupBlock.modelData.notifications)
                                }
                            }
                        }

                        Repeater {
                            model: groupBlock.modelData.notifications

                            NotificationCard {
                                id: row
                                required property var modelData

                                width: groupBlock.width
                                notification: row.modelData
                                // The full card: body, actions and the reply
                                // field. Nothing here times out, so nothing has
                                // to be clamped to a glance.
                                compact: false
                                exiting: centre.isClosing(row.modelData)

                                onCloseRequested: centre.beginClose([row.modelData])
                                // Sending is answering: the row plays out and the
                                // notification is dismissed with it, the same as
                                // any other close.
                                onReplied: centre.beginClose([row.modelData])
                                onReplyActiveChanged: {
                                    if (row.replyActive)
                                        centre.replyTarget = row.modelData;
                                    else if (centre.replyTarget === row.modelData)
                                        centre.replyTarget = null;
                                }
                            }
                        }
                    }
                }

                Text {
                    anchors.centerIn: parent
                    visible: centre.groups.length === 0
                    text: "No Notifications"
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 13
                    renderType: Text.NativeRendering
                }
            }
        }
    }

    Shortcut {
        sequence: "Escape"
        onActivated: centre.dismissed()
    }
}
