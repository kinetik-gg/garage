import Quickshell
import Quickshell.Hyprland
import Quickshell.Services.Notifications
import Quickshell.Wayland
import QtQuick

// The toast stack: one layer surface per screen, top right, showing what the
// daemon says is worth interrupting for.
//
// A Scope rather than a LazyLoader. The windows are cheap while empty -- an
// invisible layer surface is not mapped and takes no input -- and something has to
// be listening the moment the daemon emits, so there is nothing to defer.
Scope {
    id: popups

    // The overflow pill asks for the notification centre; the shell owns the
    // surface, so this only names the screen the request came from.
    signal openCenterRequested(string screenName)

    readonly property int maxVisible: 3
    readonly property int toastWidth: 390
    // How often a stack looks at its toasts: fine enough that a timeout lands
    // within a frame or two of where it was asked for, coarse enough that three
    // cards do not cost a wakeup each.
    readonly property int tickInterval: 100
    // How long a closing toast is left on screen before it is reaped. A frame
    // past NotificationCard's own 150 ms exit, so the card finishes what it
    // started even when nothing reports that it did.
    readonly property int exitGrace: 170

    // Toast lifetimes. A sender that asks for a positive duration knows something
    // the shell does not, so that request wins.
    //
    // A zero expire timeout is spec for "never expire", and browsers send it on
    // every web notification -- which turned each one into a banner that sat on
    // screen until clicked. The popup is the announcement, the centre is the
    // record: a timed-out toast collapses into the notification centre rather
    // than being cleared, so nothing is lost by refusing the request. Only
    // critical urgency keeps a sticky popup -- that is the one place urgency
    // changes behaviour: do-not-disturb still silences them (see
    // NotificationDaemon), but a critical notification the user is allowed to
    // see does not time out from under them.
    function timeoutFor(notification) {
        if (notification.expireTimeout > 0)
            // Clamped to a minute. A popup is the corner of somebody's screen, and
            // a sender that asks to keep it for an hour is either mistaken or
            // sending the wire's milliseconds where this property reports seconds.
            // Anything that genuinely has to stay is what the history is for.
            return Math.min(Math.max(notification.expireTimeout, 1), 60) * 1000;
        if (notification.urgency === NotificationUrgency.Critical)
            return 0;
        if (notification.urgency === NotificationUrgency.Low)
            return 3000;
        return 5000;
    }

    // Toasts appear on the screen the user is looking at, which is the focused
    // monitor at the moment the notification arrives. A toast then stays on the
    // screen it was given: following the pointer would move a card out from under
    // the hand reaching for its button.
    function dispatch(notification) {
        const focused = Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : "";
        let fallback = null;
        for (let index = 0; index < screenStacks.instances.length; ++index) {
            const stack = screenStacks.instances[index];
            if (fallback === null)
                fallback = stack;
            if (stack.screenName === focused) {
                stack.accept(notification);
                return;
            }
        }

        if (fallback !== null) {
            fallback.accept(notification);
            return;
        }

        // No screens to show it on. The notification is still in history, but
        // nothing will end a transient one, so end it here.
        NotificationDaemon.toastFinished(notification);
    }

    Connections {
        target: NotificationDaemon
        function onToastRequested(notification) {
            popups.dispatch(notification);
        }
    }

    // One live toast. A real object rather than a plain JS record so the countdown,
    // the hover pause and the closing flag are properties the delegates can bind
    // to, and so that an entry keeps its identity -- and its remaining time --
    // when the stack above it shrinks and it moves up a slot.
    component Toast: QtObject {
        property var notification: null
        property real remaining: 0
        property bool sticky: false
        property bool hovered: false
        property bool closing: false
        // When the card will have finished playing out. Carried by the entry
        // because entries move between slots while they are closing, and a card
        // that has already played out does not play out again for whatever lands
        // in it: a deadline the entry owns is honoured wherever it ends up.
        property real closeAt: 0
        // Whether the notification itself should be closed when the toast ends.
        // The user clicking the card's x means dismiss it; a toast simply timing
        // out means only that the popup is over -- the notification stays in
        // history, which is what a notification centre is for.
        property bool dismissOnEnd: false
    }

    Variants {
        id: screenStacks
        model: Quickshell.screens

        PanelWindow {
            id: stack
            required property var modelData

            readonly property string screenName: modelData ? modelData.name : ""
            property var toasts: []
            // Notifications that arrived with the stack already full. Counted
            // rather than queued: queueing a burst would trickle cards across the
            // screen for half a minute, and every one of them is in history
            // already. The pill says how many are waiting there.
            property int overflow: 0

            function accept(notification) {
                if (stack.toasts.length >= popups.maxVisible) {
                    stack.overflow += 1;
                    NotificationDaemon.toastFinished(notification);
                    return;
                }

                const timeout = popups.timeoutFor(notification);
                const entry = toastFactory.createObject(stack, {
                    notification: notification,
                    remaining: timeout,
                    sticky: timeout <= 0
                });
                // Newest on top.
                stack.toasts = [entry].concat(stack.toasts);
            }

            // Start a toast on its way out. The card is watching closing and plays
            // itself out; the tick below reaps the entry when the deadline passes,
            // whether or not the card got as far as saying it had finished.
            function beginClose(entry, dismiss) {
                if (!entry || entry.closing)
                    return;
                entry.dismissOnEnd = dismiss;
                entry.closeAt = Date.now() + popups.exitGrace;
                entry.closing = true;
            }

            function finish(entry) {
                if (!entry)
                    return;

                const notification = entry.notification;
                const dismiss = entry.dismissOnEnd;
                // Whether there is still a notification to act on: prune() reaches
                // here for toasts whose notification has already been closed by
                // somebody else, and a destroyed one reads as absent rather than
                // throwing when it is asked to dismiss itself.
                const live = NotificationDaemon.notifications.values.indexOf(notification) !== -1;

                // Out of the list before it is destroyed, so the delegates rebind
                // to what is left rather than to a dead object.
                stack.toasts = stack.toasts.filter(candidate => candidate !== entry);
                if (stack.toasts.length === 0)
                    stack.overflow = 0;
                entry.destroy();

                if (!live)
                    return;
                if (dismiss)
                    notification.dismiss();
                else
                    NotificationDaemon.toastFinished(notification);
            }

            // A notification can leave without its toast: the sender closes it, an
            // action destroys it, the centre clears it, the history cap evicts it.
            // Watching the tracked model catches all of those at once, which a
            // per-notification signal connection would not -- those outlive the
            // window on a screen that gets unplugged.
            function prune() {
                const live = NotificationDaemon.notifications.values;
                for (const entry of stack.toasts.slice()) {
                    if (live.indexOf(entry.notification) === -1)
                        stack.finish(entry);
                }
            }

            screen: modelData
            // Only mapped while it owns a toast. An always-visible surface would
            // take pointer input over its whole rectangle and swallow clicks meant
            // for the window underneath, and there is no content to justify it.
            visible: stack.toasts.length > 0
            implicitWidth: popups.toastWidth
            // Sized to its content, deliberately without animating the collapse: a
            // layer surface that changes height animates by being resized every
            // frame, and the cards already fade themselves out.
            implicitHeight: Math.max(1, column.implicitHeight)
            color: "transparent"
            focusable: false
            aboveWindows: true
            exclusiveZone: 0
            surfaceFormat.opaque: false

            anchors {
                top: true
                right: true
            }

            // Overlay surfaces already begin below Waybar's exclusive zone, so the
            // gutter is measured from there and not from the top of the screen.
            margins.top: Theme.windowGutter
            margins.right: Theme.windowGutter

            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "garage-notifications"
            // Cards are clicked, never typed into, and a toast that took the
            // keyboard would steal a keystroke from whatever the user was typing
            // in when it arrived.
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

            Connections {
                target: NotificationDaemon.notifications
                function onValuesChanged() {
                    stack.prune();
                }
            }

            // One clock for the whole stack, running only while it has toasts. The
            // countdown and the reaping both live on the entries, so a card that
            // moves up a slot keeps the time it had left and a card that is on its
            // way out keeps its deadline -- neither is the slot's business.
            Timer {
                interval: popups.tickInterval
                repeat: true
                running: stack.toasts.length > 0

                onTriggered: {
                    const moment = Date.now();
                    for (const entry of stack.toasts.slice()) {
                        if (entry.closing) {
                            if (moment >= entry.closeAt)
                                stack.finish(entry);
                            continue;
                        }
                        // Paused under the pointer: a toast being read is a toast
                        // being used.
                        if (entry.sticky || entry.hovered)
                            continue;
                        entry.remaining -= popups.tickInterval;
                        if (entry.remaining <= 0)
                            stack.beginClose(entry, false);
                    }
                }
            }

            Column {
                id: column
                width: parent.width
                spacing: 8

                Repeater {
                    // A fixed set of slots rather than one delegate per toast. A
                    // Repeater over a JavaScript array rebuilds every delegate
                    // whenever the array is reassigned, so each arrival would
                    // restart the appear animation of every card already up.
                    model: popups.maxVisible

                    Item {
                        id: slot
                        required property int index

                        readonly property var entry: slot.index < stack.toasts.length
                            ? stack.toasts[slot.index] : null
                        readonly property var notification: slot.entry
                            ? slot.entry.notification : null

                        width: column.width
                        height: cardLoader.active ? cardLoader.implicitHeight : 0
                        visible: cardLoader.active

                        // Passive, so it reports hover over the card's own buttons
                        // instead of competing with them for it.
                        HoverHandler {
                            id: hover
                        }

                        // Bound rather than written from onHoveredChanged: when a
                        // toast moves up a slot the pointer may no longer be over
                        // it, and hover would not *change* to say so -- the entry
                        // would keep the pause it inherited and never time out.
                        // A binding re-applies the current hover state to whichever
                        // entry the slot is holding.
                        Binding {
                            target: slot.entry
                            property: "hovered"
                            value: hover.hovered
                            when: slot.entry !== null
                            restoreMode: Binding.RestoreNone
                        }

                        Loader {
                            id: cardLoader
                            // Only rebuilt when the slot goes from empty to filled.
                            // While it stays filled the card rebinds to whichever
                            // toast moved into the slot, so nothing flashes.
                            active: slot.entry !== null
                            width: slot.width

                            sourceComponent: NotificationCard {
                                compact: true
                                notification: slot.notification
                                exiting: slot.entry !== null && slot.entry.closing
                                onCloseRequested: stack.beginClose(slot.entry, true)
                                // Only ever the entry that was on its way out: a
                                // card whose slot has meanwhile been handed a live
                                // toast is not reporting on that one. The tick
                                // reaps anything this misses.
                                onExited: {
                                    if (slot.entry && slot.entry.closing)
                                        stack.finish(slot.entry);
                                }
                            }
                        }
                    }
                }

                // What the cap left out. Clicking it opens the notification
                // centre on this screen, where the rest of the stack lives.
                ContinuousRectangle {
                    width: column.width
                    height: 26
                    visible: stack.overflow > 0
                    radius: Theme.controlRadius
                    color: overflowHover.hovered ? Theme.hover : Theme.contentTint
                    borderWidth: 1
                    borderColor: Theme.frameOuter

                    Text {
                        anchors.centerIn: parent
                        text: "+" + stack.overflow + " more"
                        color: Theme.textMuted
                        font.family: Theme.sans
                        font.pixelSize: 11
                        font.weight: Font.Medium
                        renderType: Text.NativeRendering
                    }

                    HoverHandler { id: overflowHover }
                    TapHandler {
                        onTapped: popups.openCenterRequested(stack.screen ? stack.screen.name : "")
                    }
                }
            }
        }
    }

    Component {
        id: toastFactory
        Toast {}
    }
}
