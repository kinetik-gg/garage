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

    // Which stacks the user has opened, by app name -- by name rather than by
    // group object, because the groups are rebuilt from scratch every time the
    // history changes and an opened stack has to survive the next arrival.
    //
    // Per-open by construction: the centre is destroyed when it closes, so every
    // stack is closed again the next time it is opened. That is deliberate. A
    // notification centre is read in one sitting, and a stack the user opened
    // last week is not a preference to restore.
    property var expandedGroups: ({})

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

    // Stack geometry. A closed group draws its newest card over the edges of the
    // ones behind it: each layer down starts peekOffset lower and is peekInset
    // narrower on each side, so what shows below the top card is a card's corner
    // rather than a shadow. Two layers is the whole vocabulary -- one says
    // "there is more", two says "there is more than one more", and a third
    // would be drawn entirely underneath the second.
    readonly property int peekOffset: 5
    readonly property int peekInset: 3
    readonly property int peekLimit: 2

    function isExpanded(name) {
        return centre.expandedGroups[String(name)] === true;
    }

    function toggleExpanded(name) {
        const key = String(name);
        // Copied and reassigned rather than mutated: a mutated object does not
        // notify, and every delegate reads its open state off this one.
        const next = ({});
        for (const existing in centre.expandedGroups)
            next[existing] = centre.expandedGroups[existing];
        if (next[key] === true)
            delete next[key];
        else
            next[key] = true;
        centre.expandedGroups = next;
    }

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === centre.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
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
                group = { name: name, newest: 0, notifications: [] };
                byName[name] = group;
                order.push(group);
            }

            group.notifications.push(notification);
            const arrival = NotificationDaemon.arrivalTime(notification);
            if (arrival > group.newest)
                group.newest = arrival;
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
        centre.forgetClosedStacks();
        centre.refreshPending = false;
    }

    // Drop the open state of groups that are no longer there. Without this, an
    // app whose stack was opened and then cleared would come back opened the
    // next time it sent something, which is not a state the user left it in.
    function forgetClosedStacks() {
        const kept = ({});
        let changed = false;
        for (const key in centre.expandedGroups) {
            let found = false;
            for (const group of centre.groups) {
                if (group.name === key) {
                    found = true;
                    break;
                }
            }
            if (found)
                kept[key] = centre.expandedGroups[key];
            else
                changed = true;
        }
        // Only when something actually went: every write here re-evaluates the
        // open state of every delegate in the list.
        if (changed)
            centre.expandedGroups = kept;
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

    // Top-to-bottom entrance, shared with every other palette. See PanelMotion.
    PanelMotion {
        id: motion
        onFinished: centre.dismissed()
    }

    function requestDismissal() {
        motion.dismiss();
    }

    anchors {
        top: true
        right: true
        bottom: true
    }

    // The animated top margin, made an integer once rather than at each margin.
    //
    // Margins are ints and PanelMotion's travel is a real, so letting the two
    // margins below each convert it themselves is two conversions of a moving
    // fractional value that do not always sum to the constant the compensation
    // depends on: at a top of 3.7 they come out 3 and 8, which is 11 rather than
    // 12 and so a surface one pixel taller than it was the frame before. A pixel
    // is not the stretch this arrangement was written to stop, but it is a
    // relayout of the list, and those are the whole cost. One integer, used
    // twice, and the pair sums to the same number on every frame.
    readonly property int surfaceTopPx: Math.round(motion.surfaceTop)

    // Overlay surfaces already begin below Waybar's exclusive zone, so the top
    // gutter is measured from there rather than from the top of the screen.
    margins.top: centre.surfaceTopPx
    margins.right: Theme.windowGutter
    // The bottom margin moves with the top, by the same amount in the opposite
    // direction, so the entrance translates this surface instead of resizing it.
    //
    // It is anchored top and bottom because the list wants the full height of
    // the output. That makes margins.top a size as well as a position: animated
    // on its own it stretched a 390x1025 surface on every frame of the
    // entrance, and each of those frames was a full relayout of the notification
    // list and a re-blur of the whole thing. Holding the height constant is what
    // makes this cost a move rather than sixty relayouts a second.
    margins.bottom: Theme.windowGutter * 2 - centre.surfaceTopPx

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
        opacity: motion.opacity
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

                    // One app's notifications, as a stack. Closed, the newest
                    // card stands over the edges of the ones behind it and the
                    // whole thing is one control: click it and the group opens
                    // in place. There is no header row -- the stack is the
                    // group, and every card carries the app's icon and name
                    // itself.
                    delegate: Column {
                        id: groupBlock
                        required property var modelData

                        readonly property var items: groupBlock.modelData.notifications
                        readonly property int count: groupBlock.items.length
                        // One notification is not a stack, it is a card: an edge
                        // peeking out from under it would be promising something
                        // that is not there.
                        readonly property bool stacked: groupBlock.count > 1
                        readonly property bool open: groupBlock.stacked
                            && centre.isExpanded(groupBlock.modelData.name)
                        readonly property bool closed: groupBlock.stacked && !groupBlock.open
                        readonly property int peeks: groupBlock.closed
                            ? Math.min(centre.peekLimit, groupBlock.count - 1) : 0

                        width: list.width
                        spacing: 6

                        Item {
                            width: groupBlock.width
                            height: cards.height + groupBlock.peeks * centre.peekOffset

                            // Under the cards rather than over them. A card's own
                            // buttons take the clicks that are theirs -- the x, an
                            // action -- and everything they leave alone falls
                            // through to here, which is what lets the stack be
                            // the control that opens the stack.
                            MouseArea {
                                anchors.fill: parent
                                enabled: groupBlock.closed
                                hoverEnabled: enabled
                                cursorShape: Qt.PointingHandCursor
                                onClicked: centre.toggleExpanded(groupBlock.modelData.name)
                            }

                            // The edges of what is underneath. Drawn rather than
                            // instantiated: these are the backs of cards nobody
                            // is reading, and real ones would mean a second live
                            // card for every notification in the group, each with
                            // its own retain lock and exit animation to keep in
                            // step with the copy that appears when the stack opens.
                            Repeater {
                                model: groupBlock.peeks

                                ContinuousRectangle {
                                    id: peek
                                    required property int index

                                    readonly property int level: peek.index + 1

                                    x: peek.level * centre.peekInset
                                    y: peek.level * centre.peekOffset
                                    width: groupBlock.width - peek.level * centre.peekInset * 2
                                    // As tall as the card in front, so the only
                                    // thing that shows is the offset.
                                    height: cards.height
                                    radius: Theme.cornerRadius
                                    color: Theme.contentTint
                                    borderWidth: 1
                                    // The card's own outer frame is a dark
                                    // hairline, which is invisible where the only
                                    // thing behind it is more dark: what shows of
                                    // a layer down is four pixels over blurred
                                    // wallpaper, so it is drawn with the lighter
                                    // control border instead.
                                    borderColor: Theme.border
                                    // Dimmed by depth: the second layer down is
                                    // further away than the first. Gently -- the
                                    // sliver is thin enough that much more of a
                                    // fade stops reading as a card at all.
                                    opacity: 1 - peek.level * 0.15
                                }
                            }

                            Column {
                                id: cards

                                width: groupBlock.width
                                spacing: 8

                                Repeater {
                                    // The newest alone while the stack is closed.
                                    // Opening one does not stand up a second copy
                                    // of the card already on screen: the top row
                                    // stays where it is and the rest arrive
                                    // underneath it.
                                    model: groupBlock.closed
                                        ? [groupBlock.items[0]] : groupBlock.items

                                    NotificationCard {
                                        id: row
                                        required property var modelData

                                        width: cards.width
                                        notification: row.modelData
                                        // The full card: body, actions and the
                                        // reply field. Nothing here times out, so
                                        // nothing has to be clamped to a glance --
                                        // except the top of a closed stack, which
                                        // is standing in for the rows behind it.
                                        compact: false
                                        collapsed: groupBlock.closed
                                        stackNote: groupBlock.closed
                                            ? (groupBlock.count - 1) + " more" : ""
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

                        // The open stack's footer, and the only place the retired
                        // group header's two jobs survive: the way back, and
                        // clearing the group in one go. Closed, neither is needed
                        // -- the stack opens on a click and the count in the top
                        // card already says how much is under it.
                        Item {
                            width: groupBlock.width
                            height: showLess.implicitHeight
                            visible: groupBlock.open

                            SettingsButton {
                                id: showLess
                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Show Less"
                                ghost: true
                                verticalPadding: 4
                                horizontalPadding: 8
                                onClicked: centre.toggleExpanded(groupBlock.modelData.name)
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
                                    onClicked: centre.beginClose(groupBlock.items)
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

    // Through the motion, not straight to dismissed(): the signal is what makes
    // the shell destroy this window, so raising it here would take the panel off
    // screen on the frame Escape lands and leave the exit with nothing to play.
    Shortcut {
        sequence: "Escape"
        onActivated: centre.requestDismissal()
    }
}
