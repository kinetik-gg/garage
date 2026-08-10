import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import QtQuick

ShellRoot {
    id: shell
    property string sessionScreenName: ""
    property string preferencesScreenName: ""
    property string preferencesSection: "general"
    property string notificationScreenName: ""
    property string controlCenterScreenName: ""
    property string launcherScreenName: ""
    property string monitorScreenName: ""
    property string mediaScreenName: ""
    property string aiUsageScreenName: ""

    // Which transient surface is on screen, by name, or "" for none.
    //
    // The transient surfaces -- the launcher, the session menu, the notification
    // centre, the control centre and the three panels the bar's own modules open
    // (the activity monitor, the now-playing panel and the AI usage panel) -- are
    // layer overlays that hold the keyboard or the pointer for as long as they are
    // up, so no two of them can usefully be on screen together. Holding that as
    // one name rather than a boolean per loader is what makes it true by
    // construction: every loader binds to this, so activating one deactivates the
    // rest with no cross-loader clears to keep in step. Those clears were written
    // out at each entry point before, and were, in places, forgotten -- session()
    // closed the screenshot pill and about() closed nothing.
    //
    // Deliberately not the screenshot pill, which used to be in here: see
    // screenshotOpen below.
    //
    // Deliberately not the Preferences and About windows: those are
    // FloatingWindows the compositor stacks like any other application window,
    // and closing one because the user pressed the launcher key would throw away
    // whatever they had open in it. Opening one still dismisses the transient
    // that raised it, because the click that opened it was a click on that menu.
    //
    // Nor the notification popups: those are the shell speaking rather than the
    // user opening something, they never take input focus, and they have to be
    // able to appear over anything in this list.
    property string activeSurface: ""

    // The screenshot pill, on a flag of its own rather than in activeSurface.
    //
    // Everything in that set closes everything else in it, and the pill was in it
    // -- so pressing the screenshot bind with the notification centre or the
    // control centre open dismissed the panel the user had opened the pill to
    // photograph. The pill is the one surface whose purpose is what is already on
    // screen, so it is the one surface that cannot be mutually exclusive with
    // everything.
    //
    // It keeps the rest of its pairings: the launcher and the session menu hold
    // the keyboard and are nothing to photograph, so they and the pill still close
    // each other, and opening Preferences or About still dismisses it. Those
    // pairings live in the four functions below rather than at each entry point.
    property bool screenshotOpen: false

    function focusedScreenName() {
        return Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : "";
    }

    // The surfaces the pill may share the screen with: the panels worth
    // taking a picture of. All of them stay up through the capture, which is
    // what disarming the dismiss catcher below is for.
    function sharesScreenWithScreenshot(name) {
        return name === "notificationCenter" || name === "controlCenter"
            || name === "monitorPalette" || name === "mediaPalette"
            || name === "aiPalette";
    }

    // The surfaces whose click-outside dismissal the shared catcher owns.
    //
    // Not the session menu: it has its own dismissal, a non-consuming mouse
    // bind that runs garage-menu-dismiss and compares the cursor against the
    // menu's own box, and that one works. Not the screenshot pill either --
    // the catcher stands down for the pill rather than guarding it.
    function catchesOutsideClicks(name) {
        return name === "launcher"
            || name === "notificationCenter"
            || name === "controlCenter"
            || name === "monitorPalette"
            || name === "mediaPalette"
            || name === "aiPalette";
    }

    // Whether a palette that is up has asked not to be dismissed out from under
    // a gesture still in progress.
    //
    // MonitorPalette and MediaPalette each carry a holdOpen for this, and each
    // documents the shell binding the shared catcher's `armed` to it: the live
    // case is MediaPalette's seek bar, where the pointer leaves the panel during
    // the drag and the release is the tail of that drag rather than the user
    // clicking away. Read through the loaders' items because the palettes only
    // exist while they are up. AiUsagePalette has no holdOpen -- there is no
    // gesture in it to interrupt -- so it is not asked.
    function paletteHoldsOpen() {
        if (monitorPaletteLoader.item && monitorPaletteLoader.item.holdOpen)
            return true;
        if (mediaPaletteLoader.item && mediaPaletteLoader.item.holdOpen)
            return true;
        return false;
    }

    // configure() runs before the surface is shown, so the palette is created
    // with the screen and section it is meant to open on rather than being moved
    // a frame later.
    function openSurface(name, configure) {
        if (configure)
            configure();
        if (!shell.sharesScreenWithScreenshot(name))
            shell.screenshotOpen = false;
        shell.activeSurface = name;
    }

    // Same, except that asking for the surface already up closes it. configure()
    // still runs either way, which is where it ran before this was one function:
    // session() set the target screen and then toggled.
    function toggleSurface(name, configure) {
        if (configure)
            configure();
        if (shell.activeSurface === name) {
            // Closing something is not opening anything, so the pill is left
            // where it is rather than being taken down with the panel.
            shell.activeSurface = "";
            return;
        }
        if (!shell.sharesScreenWithScreenshot(name))
            shell.screenshotOpen = false;
        shell.activeSurface = name;
    }

    function closeSurface(name) {
        if (shell.activeSurface === name)
            shell.activeSurface = "";
    }

    // The pill closes whatever cannot share the screen with it, which is what
    // being in activeSurface used to do for it.
    function openScreenshot() {
        if (!shell.sharesScreenWithScreenshot(shell.activeSurface))
            shell.activeSurface = "";
        shell.screenshotOpen = true;
    }

    // Run rather than detached, for its lifetime alone: the capture is what the
    // dismiss catcher stands down for, and a detached command has no lifetime the
    // shell can see. Detaching it left the centre being photographed dismissed by
    // the tool photographing it.
    function captureScreenshot(mode) {
        const command = [
            Quickshell.env("HOME") + "/.local/bin/garage-screenshot-copy",
            mode
        ];
        if (captureProcess.running) {
            // A capture already running is already what the panels are holding
            // open for. Detach the second one rather than move that goalpost.
            Quickshell.execDetached(command);
            return;
        }
        captureProcess.command = command;
        captureProcess.running = true;
    }

    // What a session action runs, in one place, named by the session menu -- the
    // one surface that offers the session commands. The control centre offered a
    // second copy of Lock, Sleep and Log Out through here until they were taken
    // out of it; the table stays here rather than in SessionPalette so the next
    // caller has somewhere to name them from instead of writing its own map.
    function runSessionAction(action) {
        const commands = {
            "reloadHyprland": ["hyprctl", "reload"],
            "lock": ["hyprlock"],
            "logout": ["uwsm", "stop"],
            "suspend": ["systemctl", "suspend"],
            "hibernate": ["systemctl", "hibernate"],
            "restart": ["systemctl", "reboot"],
            "poweroff": ["systemctl", "poweroff"]
        };

        if (commands[action] !== undefined)
            Quickshell.execDetached(commands[action]);
    }

    function openWindow(loader, configure) {
        if (configure)
            configure();
        // Whatever transient raised this window goes with the click that raised
        // it, the pill included. The window itself is left alone by every other
        // surface.
        shell.activeSurface = "";
        shell.screenshotOpen = false;
        loader.active = true;
    }

    function toggleWindow(loader, configure) {
        if (configure)
            configure();
        if (loader.active) {
            loader.active = false;
            return;
        }
        shell.activeSurface = "";
        shell.screenshotOpen = false;
        loader.active = true;
    }

    // Click-outside dismissal for the launcher, the two centres and the three
    // panels the bar opens, in one place rather than one copy per palette: they
    // all want the same gesture, they are all mutually exclusive by
    // activeSurface, and a single catcher therefore never has to work out which
    // of them it is guarding -- it closes whatever is up.
    //
    // Declared before the palette loaders on purpose. This and they both bind
    // to activeSurface, so within the turn that changes it they are served in
    // declaration order -- the catcher's surfaces map first, the palette's map
    // second, and on this compositor the surface mapped second is the one that
    // stacks on top. Measured. Moving this below them would put the catcher
    // over the palette and turn every click on the panel into a dismissal.
    DismissCatcher {
        active: shell.catchesOutsideClicks(shell.activeSurface)
        // Stood down for the whole screenshot flow: the pill's own clicks are
        // not the user clicking away from the panel being photographed, and
        // neither are slurp's while a region is being dragged out. Stood down
        // the same way for a palette holding itself open through a gesture --
        // see paletteHoldsOpen above.
        armed: !(shell.screenshotOpen || captureProcess.running
            || shell.paletteHoldsOpen())
        onDismissed: shell.closeSurface(shell.activeSurface)
    }

    LazyLoader {
        id: preferencesLoader
        active: false

        PreferencesPalette {
            targetScreenName: shell.preferencesScreenName
            initialSection: shell.preferencesSection
            onDismissed: preferencesLoader.active = false
        }
    }

    LazyLoader {
        id: aboutLoader
        active: false

        AboutPalette {
            targetScreenName: shell.sessionScreenName
            onDismissed: aboutLoader.active = false
            onMoreInfoRequested: shell.openWindow(preferencesLoader, () => {
                shell.preferencesScreenName = shell.sessionScreenName;
                shell.preferencesSection = "about";
            })
        }
    }

    LazyLoader {
        id: launcherLoader
        active: shell.activeSurface === "launcher"

        LauncherPalette {
            targetScreenName: shell.launcherScreenName
            onDismissed: shell.closeSurface("launcher")
        }
    }

    LazyLoader {
        id: screenshotLoader
        active: shell.screenshotOpen

        ScreenshotPalette {
            onModeSelected: mode => {
                // The pill goes first: it is a layer surface over the desktop, and
                // grim would otherwise photograph it along with everything else.
                shell.screenshotOpen = false;
                shell.captureScreenshot(mode);
            }

            onDismissed: shell.screenshotOpen = false
        }
    }

    // The capture itself. Its lifetime is what the dismiss catcher stands down
    // for, so it has to be a Process the shell can see rather than a detached
    // command.
    Process {
        id: captureProcess
    }

    LazyLoader {
        id: sessionLoader
        active: shell.activeSurface === "session"

        SessionPalette {
            targetScreenName: shell.sessionScreenName

            onActionSelected: action => {
                shell.closeSurface("session");

                if (action === "about") {
                    shell.openWindow(aboutLoader);
                    return;
                }

                if (action === "preferences") {
                    shell.openWindow(preferencesLoader, () => {
                        shell.preferencesScreenName = shell.sessionScreenName;
                        shell.preferencesSection = "general";
                    });
                    return;
                }

                shell.runSessionAction(action);
            }

            onDismissed: shell.closeSurface("session")
        }
    }

    LazyLoader {
        id: notificationCenterLoader
        active: shell.activeSurface === "notificationCenter"

        NotificationCenterPalette {
            targetScreenName: shell.notificationScreenName
            onDismissed: shell.closeSurface("notificationCenter")
        }
    }

    LazyLoader {
        id: controlCenterLoader
        active: shell.activeSurface === "controlCenter"

        ControlCenterPalette {
            targetScreenName: shell.controlCenterScreenName
            onDismissed: shell.closeSurface("controlCenter")
        }
    }

    // The three panels the bar's own modules open, on the same shape as the two
    // centres above: one loader each, bound to activeSurface, and a dismissed()
    // that clears the name it was activated by. The monitor and the AI usage
    // panel sit against the right edge like the centres; the media panel is
    // anchored to the top edge alone, so the compositor centres it under the bar.
    LazyLoader {
        id: monitorPaletteLoader
        active: shell.activeSurface === "monitorPalette"

        MonitorPalette {
            targetScreenName: shell.monitorScreenName
            onDismissed: shell.closeSurface("monitorPalette")
        }
    }

    LazyLoader {
        id: mediaPaletteLoader
        active: shell.activeSurface === "mediaPalette"

        MediaPalette {
            targetScreenName: shell.mediaScreenName
            onDismissed: shell.closeSurface("mediaPalette")
        }
    }

    LazyLoader {
        id: aiPaletteLoader
        active: shell.activeSurface === "aiPalette"

        AiUsagePalette {
            targetScreenName: shell.aiUsageScreenName
            onDismissed: shell.closeSurface("aiPalette")
        }
    }

    // The notification service, and the only surface in the shell that is not
    // loaded on demand. Everything else here is opened by the user, so it can wait
    // until they ask; a notification server has to be on the bus before anything
    // sends to it, and nothing in a LazyLoader exists until something activates it.
    // Reaching NotificationDaemon from here is also what brings the singleton --
    // and with it org.freedesktop.Notifications -- up with the shell.
    NotificationPopups {
        onOpenCenterRequested: screenName => {
            shell.notificationScreenName = screenName;
            shell.openSurface("notificationCenter", () => {});
        }
    }

    // Whether the shell's own launcher is switched on. SUPER+Space already asks
    // the same marker through its wrapper; this closes the other routes in, so
    // nothing can open a launcher the user turned off.
    FileView {
        id: launcherMode
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/launcher"
        printErrors: false
        // Read up front and re-read on every change: watchChanges reports the
        // write but does not pull it in, and an unloaded FileView reads as an
        // empty file, which is the answer that leaves the launcher switched on.
        blockLoading: true
        watchChanges: true
        onFileChanged: reload()
    }

    IpcHandler {
        target: "shell"
        property bool screenshotVisible: screenshotLoader.active
        property bool sessionVisible: sessionLoader.active
        property bool aboutVisible: aboutLoader.active
        property bool preferencesVisible: preferencesLoader.active
        property bool launcherVisible: launcherLoader.active
        property bool notificationCenterVisible: notificationCenterLoader.active
        property bool controlCenterVisible: controlCenterLoader.active
        property bool monitorVisible: monitorPaletteLoader.active
        property bool mediaVisible: mediaPaletteLoader.active
        property bool aiUsageVisible: aiPaletteLoader.active

        function launcher(): void {
            // Read per call rather than bound: the marker changes underneath a
            // running shell, and an empty one is a session that has not
            // published the setting yet, which means the built-in launcher.
            if (String(launcherMode.text() || "").trim() === "external") {
                shell.closeSurface("launcher");
                return;
            }
            // The monitor is resolved here, on the keypress, rather than bound
            // inside the palette: the launcher is a layer surface now, and the
            // focused monitor moves with the pointer, so a binding would walk an
            // open launcher between screens.
            shell.toggleSurface("launcher", () => {
                shell.launcherScreenName = shell.focusedScreenName();
            });
        }

        function closeLauncher(): void {
            shell.closeSurface("launcher");
        }

        function screenshot(): void {
            shell.openScreenshot();
        }

        function closeScreenshot(): void {
            shell.screenshotOpen = false;
        }

        function session(): void {
            shell.toggleSurface("session", () => {
                shell.sessionScreenName = shell.focusedScreenName();
            });
        }

        function sessionOn(screenName: string): void {
            // Asked for again on the screen it is already on, it closes; asked
            // for on another screen, it moves there rather than closing under a
            // pointer that is somewhere else entirely.
            const sameScreen = shell.sessionScreenName === screenName;
            const open = () => {
                shell.sessionScreenName = screenName;
            };
            if (sameScreen)
                shell.toggleSurface("session", open);
            else
                shell.openSurface("session", open);
        }

        function closeSession(): void {
            shell.closeSurface("session");
        }

        function closeAbout(): void {
            aboutLoader.active = false;
        }

        function preferences(): void {
            shell.toggleWindow(preferencesLoader, () => {
                shell.preferencesScreenName = shell.focusedScreenName();
                shell.preferencesSection = "general";
            });
        }

        function preferencesOn(section: string): void {
            shell.openWindow(preferencesLoader, () => {
                shell.preferencesScreenName = shell.focusedScreenName();
                shell.preferencesSection = section;
            });
        }

        function closePreferences(): void {
            preferencesLoader.active = false;
        }

        function about(): void {
            shell.toggleWindow(aboutLoader, () => {
                shell.sessionScreenName = shell.focusedScreenName();
            });
        }

        function aboutOn(screenName: string): void {
            shell.openWindow(aboutLoader, () => {
                shell.sessionScreenName = screenName;
            });
        }

        // The centre is a surface like the session menu, so it opens and closes
        // here rather than on the "notifications" handler below: that one is the
        // notification service's controls, and garage-lock-session already calls
        // `shell closeNotifications` on its way to the lock screen.
        function notifications(): void {
            shell.toggleSurface("notificationCenter", () => {
                shell.notificationScreenName = shell.focusedScreenName();
            });
        }

        function notificationsOn(screenName: string): void {
            const sameScreen = shell.notificationScreenName === screenName;
            const open = () => {
                shell.notificationScreenName = screenName;
            };
            if (sameScreen)
                shell.toggleSurface("notificationCenter", open);
            else
                shell.openSurface("notificationCenter", open);
        }

        function closeNotifications(): void {
            shell.closeSurface("notificationCenter");
        }

        // The control centre, on the same three-function shape as the centre
        // above: the bar's glyph and SUPER+CTRL+A both come in through
        // garage-panel-toggle, which resolves the monitor under the pointer and
        // calls controlCenterOn with it.
        function controlCenter(): void {
            shell.toggleSurface("controlCenter", () => {
                shell.controlCenterScreenName = shell.focusedScreenName();
            });
        }

        function controlCenterOn(screenName: string): void {
            const sameScreen = shell.controlCenterScreenName === screenName;
            const open = () => {
                shell.controlCenterScreenName = screenName;
            };
            if (sameScreen)
                shell.toggleSurface("controlCenter", open);
            else
                shell.openSurface("controlCenter", open);
        }

        function closeControlCenter(): void {
            shell.closeSurface("controlCenter");
        }

        // The three bar panels, each on the same three-function shape as the
        // control centre above. The click on a bar module goes through
        // garage-panel-toggle, which resolves the monitor under the pointer and
        // calls the *On form with it; the parameterless form is there for a
        // keybind, which has no pointer to resolve and so takes the focused
        // monitor, the way launcher() and session() do.
        function monitor(): void {
            shell.toggleSurface("monitorPalette", () => {
                shell.monitorScreenName = shell.focusedScreenName();
            });
        }

        function monitorOn(screenName: string): void {
            const sameScreen = shell.monitorScreenName === screenName;
            const open = () => {
                shell.monitorScreenName = screenName;
            };
            if (sameScreen)
                shell.toggleSurface("monitorPalette", open);
            else
                shell.openSurface("monitorPalette", open);
        }

        function closeMonitor(): void {
            shell.closeSurface("monitorPalette");
        }

        function media(): void {
            shell.toggleSurface("mediaPalette", () => {
                shell.mediaScreenName = shell.focusedScreenName();
            });
        }

        function mediaOn(screenName: string): void {
            const sameScreen = shell.mediaScreenName === screenName;
            const open = () => {
                shell.mediaScreenName = screenName;
            };
            if (sameScreen)
                shell.toggleSurface("mediaPalette", open);
            else
                shell.openSurface("mediaPalette", open);
        }

        function closeMedia(): void {
            shell.closeSurface("mediaPalette");
        }

        function aiUsage(): void {
            shell.toggleSurface("aiPalette", () => {
                shell.aiUsageScreenName = shell.focusedScreenName();
            });
        }

        function aiUsageOn(screenName: string): void {
            const sameScreen = shell.aiUsageScreenName === screenName;
            const open = () => {
                shell.aiUsageScreenName = screenName;
            };
            if (sameScreen)
                shell.toggleSurface("aiPalette", open);
            else
                shell.openSurface("aiPalette", open);
        }

        function closeAiUsage(): void {
            shell.closeSurface("aiPalette");
        }
    }

    // A handler of its own rather than more functions on "shell": these are the
    // notification service's controls, and the callers are different -- garage for
    // the preferences switch, hyprlock and the recorder for the inhibitors.
    IpcHandler {
        target: "notifications"
        property bool dnd: NotificationDaemon.dnd

        function setDnd(value: bool): void {
            NotificationDaemon.setDnd(value);
        }

        function toggleDnd(): void {
            NotificationDaemon.toggleDnd();
        }

        // Named holds, so a caller that arms twice cannot leave the shell silent
        // by releasing once.
        function inhibit(name: string): void {
            NotificationDaemon.addInhibitor(name);
        }

        function release(name: string): void {
            NotificationDaemon.removeInhibitor(name);
        }

        function clear(): void {
            NotificationDaemon.clearAll();
        }
    }
}
