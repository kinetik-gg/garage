pragma Singleton

import Quickshell
import Quickshell.Io
import Quickshell.Services.Notifications
import QtQuick

// The shell's notification service: the freedesktop server, the history it keeps,
// and the two switches that decide whether anything is allowed on screen.
//
// A singleton because org.freedesktop.Notifications is a single bus name. Every
// surface that wants notifications -- the popups now, the centre and the bar
// indicator later -- reads this one, so nothing can stand up a second server and
// take the name away from the first.
Singleton {
    id: daemon

    // The history. Notifications live here until something closes them, which is
    // why a popup timing out does not call expire(): expire() and dismiss() both
    // destroy the notification, and a destroyed notification is not in a history
    // any more. A toast ending is a popup ending, nothing else.
    readonly property alias notifications: server.trackedNotifications

    // Cap on the history. The tracked model is the only copy, so an unbounded one
    // is a session-long leak: a chat client alone can send hundreds a day.
    readonly property int historyLimit: 100

    // One clock for every relative timestamp in the shell rather than a timer per
    // card: a stack of toasts and a full centre would otherwise each wake the
    // process on their own schedule to redraw the same minute.
    readonly property date now: minuteClock.date

    // Emitted for notifications that should raise a popup. Deliberately not the
    // server's own signal: this one has already been filtered for do-not-disturb,
    // for inhibitors, and for the reload replay below, so a popup surface can
    // show everything it is handed.
    signal toastRequested(var notification)

    // Do not disturb. Shell-owned because the freedesktop server has no concept
    // of it -- the protocol has no quiet-hours hint, and clients keep sending
    // regardless -- so silencing has to happen on this side of the bus. swaync
    // kept the same flag for the same reason, which is why the switch survives
    // the cutover with its meaning intact.
    //
    // Read-only from outside: it is persisted, so every write goes through
    // setDnd() and reaches the file.
    readonly property alias dnd: persisted.dnd

    // Runtime-only silencing by name, for things that are temporarily wrong to
    // interrupt: the lock screen, a fullscreen presentation, a screen recording.
    // Not persisted, and named rather than counted so a holder that asks twice
    // (a lock that re-arms, a pane that rebuilds) cannot leave the shell silent
    // for the rest of the session by releasing once.
    property var inhibitors: []

    readonly property bool popupsSuppressed: dnd || inhibitors.length > 0

    // Arrival times, keyed by notification id. Notification carries no timestamp
    // and has nothing writable to hang one on, so the daemon records it: cards
    // need an age to print, and the history cap needs an order to evict in.
    //
    // Kept here rather than in the card because a card is created and destroyed
    // as surfaces open and close -- a card-owned arrival time would reset to now
    // every time the centre was opened.
    property var arrivals: ({})

    // Re-entrancy guard for sweep(), which closes notifications and so provokes
    // the very signal that called it.
    property bool sweeping: false

    function arrivalTime(notification) {
        if (!notification)
            return Date.now();
        const recorded = arrivals[String(notification.id)];
        // Notifications carried over from a previous generation have no recorded
        // arrival -- the map is rebuilt empty on reload -- so they read as new.
        // Wrong by the age of the notification, and the alternative is persisting
        // the map, which is a notification centre problem rather than a toast one.
        return recorded === undefined ? Date.now() : recorded;
    }

    function setDnd(value) {
        const next = value === true;
        if (persisted.dnd === next)
            return;
        persisted.dnd = next;
        // JsonAdapter does not write itself back; the file only changes when
        // asked. Written on every toggle rather than at exit because a shell that
        // is killed never gets an exit.
        stateFile.writeAdapter();
    }

    function toggleDnd() {
        setDnd(!dnd);
    }

    function addInhibitor(name) {
        const key = String(name || "").trim();
        if (key === "" || inhibitors.indexOf(key) !== -1)
            return;
        // Reassigned rather than pushed: a mutated array does not notify, and
        // popupsSuppressed is a binding on its length.
        inhibitors = inhibitors.concat([key]);
    }

    function removeInhibitor(name) {
        const key = String(name || "").trim();
        if (inhibitors.indexOf(key) === -1)
            return;
        inhibitors = inhibitors.filter(entry => entry !== key);
    }

    function clearAll() {
        // Snapshot first: dismissing rewrites the model this is walking.
        for (const notification of server.trackedNotifications.values.slice())
            notification.dismiss();
    }

    // A popup has finished with this notification. Only transient notifications
    // care: the specification says they must not reach anything persistent, so
    // the toast is their whole life and it ends here.
    function toastFinished(notification) {
        if (notification && notification.transient)
            notification.dismiss();
    }

    function present(notification) {
        arrivals[String(notification.id)] = Date.now();

        // Every close is journaled with its reason (1 expired, 2 dismissed,
        // 3 closed by the sender). Deliberate, not debug leftovers: one
        // notification was once destroyed with no visible cause in a session
        // that could not be reopened, and this line is what turns the next
        // such ghost into a one-grep diagnosis. The volume is a line per
        // close -- journald noise well below the toplevel tracker's.
        notification.closed.connect(function(reason) {
            console.log("garage-notif: id", notification.id,
                        "closed, reason", reason);
        });

        // Tracking is what keeps the object alive -- an untracked notification is
        // destroyed as soon as this handler returns, and tracked = false is
        // documented as equivalent to dismiss(). Transient ones are tracked too,
        // because a popup needs a live object for the length of its toast; they
        // leave again through toastFinished(), which is what keeps them out of
        // the history they are not supposed to reach.
        notification.tracked = true;

        // keepOnReload re-emits everything the previous generation held, so the
        // shell keeps its history across a reload. Those notifications have all
        // been seen already: without this every reload would replay the whole
        // backlog across the screen.
        if (notification.lastGeneration)
            return;

        if (popupsSuppressed) {
            // Suppressed is not discarded -- the notification stays in history,
            // which is the entire difference between do-not-disturb and a mute.
            // A critical notification is silenced with the rest: swaync did not
            // exempt them either, and an urgency flag chosen by the sender is not
            // permission to override what the user asked for. It is still in the
            // history, and still critical when it is read there.
            toastFinished(notification);
            return;
        }

        toastRequested(notification);
    }

    // Prune the arrival map and enforce the history cap. Driven from the model
    // rather than from present() so evictions also follow notifications that
    // leave by any other route.
    function sweep() {
        if (sweeping)
            return;
        sweeping = true;

        const live = server.trackedNotifications.values;
        const kept = ({});
        for (const notification of live) {
            const key = String(notification.id);
            kept[key] = arrivals[key] === undefined ? Date.now() : arrivals[key];
        }
        arrivals = kept;

        let over = live.length - historyLimit;
        if (over > 0) {
            // Oldest first, and never a resident notification: resident means the
            // sender expects it to stay until it acts, so evicting one breaks a
            // progress bar or a download rather than dropping a stale banner.
            const ordered = live.slice().sort(
                (first, second) => daemon.arrivalTime(first) - daemon.arrivalTime(second));
            for (const notification of ordered) {
                if (over <= 0)
                    break;
                if (notification.resident)
                    continue;
                // Expired rather than dismissed: aged out of history is a timeout,
                // not the user closing it, and clients read the reason.
                notification.expire();
                --over;
            }
            // The model changed underneath the walk above and this sweep was the
            // one holding the guard, so the pruning pass has to be asked for
            // again. callLater coalesces, so a run of evictions costs one.
            Qt.callLater(sweep);
        }

        sweeping = false;
    }

    NotificationServer {
        id: server

        // Hand the history back after a reload instead of dropping every
        // notification the user had not read yet. present() sorts the replay out.
        keepOnReload: true
        actionsSupported: true
        bodySupported: true
        bodyMarkupSupported: true
        imageSupported: true
        // On because there is now somewhere to reply from: the notification
        // centre draws a field for anything that asks for one, and a card
        // sends through Notification.sendInlineReply() from it. Advertising a
        // capability nothing draws would make clients send repliable
        // notifications with nowhere to answer them.
        inlineReplySupported: true
        // Persistence advertises that notifications survive going offscreen with
        // the server still able to act on them. The centre makes that true, but
        // the flag is a claim clients change their own behaviour on -- several
        // drop their in-app banner when the server says it persists -- so it is
        // turned on deliberately rather than as a side effect of the centre
        // landing.
        persistenceSupported: false
        // extraHints is left alone: it widens the hints map with names the shell
        // asks for by hand, and nothing here reads a non-standard hint yet.

        onNotification: notification => daemon.present(notification)
    }

    Connections {
        target: server.trackedNotifications
        function onValuesChanged() {
            daemon.sweep();
        }
    }

    SystemClock {
        id: minuteClock
        // Relative timestamps are printed to the minute, so a second-precision
        // clock would wake the shell sixty times for every visible change.
        precision: SystemClock.Minutes
    }

    // Do-not-disturb outlives the shell: it is a setting the user changed, not a
    // detail of this process, and a reload or a logout that silently turned the
    // banners back on would be the shell overruling them.
    //
    // Directly under the state root rather than in generated/, which `garage
    // render` rewrites from the preference files and which is safe to delete:
    // nothing renders this, so it belongs beside preferences.lock instead.
    FileView {
        id: stateFile
        path: Quickshell.env("HOME") + "/.local/state/garage/notification-state.json"
        // A first run has no file, and the absence is the answer (banners on)
        // rather than a fault worth logging.
        printErrors: false
        blockLoading: true
        watchChanges: true
        onFileChanged: reload()

        JsonAdapter {
            id: persisted
            property bool dnd: false
        }
    }

    // The adapter is filled by a load, and a load is asynchronous unless a
    // blocking read forces it: preload alone lands a frame or two later, which is
    // long enough for the first notification to arrive and pop up in a session the
    // user had silenced. text() with blockLoading reads the file now and fills the
    // adapter with it, which is the same up-front read the marker FileViews in
    // shell.qml do for the same reason. The value is discarded; the side effect is
    // the point.
    //
    // It also has to happen before anything can call setDnd(): writing the adapter
    // before its first load would persist the default over the stored setting.
    Component.onCompleted: stateFile.text()
}
