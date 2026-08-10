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

    // Which transient surface is on screen, by name, or "" for none.
    //
    // The transient surfaces -- the launcher, the screenshot pill, the session
    // menu and the notification centre -- are layer overlays that hold the
    // keyboard or the pointer for as long as they are up, so no two of them can
    // usefully be on screen together. Holding that as one name rather than a
    // boolean per loader is what makes it true by construction: every loader
    // binds to this, so activating one deactivates the rest with no cross-loader
    // clears to keep in step. Those clears were written out at each entry point
    // before, and were, in places, forgotten -- session() closed the screenshot
    // pill and about() closed nothing.
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

    function focusedScreenName() {
        return Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : "";
    }

    // configure() runs before the surface is shown, so the palette is created
    // with the screen and section it is meant to open on rather than being moved
    // a frame later.
    function openSurface(name, configure) {
        if (configure)
            configure();
        shell.activeSurface = name;
    }

    // Same, except that asking for the surface already up closes it. configure()
    // still runs either way, which is where it ran before this was one function:
    // session() set the target screen and then toggled.
    function toggleSurface(name, configure) {
        if (configure)
            configure();
        shell.activeSurface = shell.activeSurface === name ? "" : name;
    }

    function closeSurface(name) {
        if (shell.activeSurface === name)
            shell.activeSurface = "";
    }

    function openWindow(loader, configure) {
        if (configure)
            configure();
        // Whatever transient raised this window goes with the click that raised
        // it. The window itself is left alone by every other surface.
        shell.activeSurface = "";
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
        loader.active = true;
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
            onDismissed: shell.closeSurface("launcher")
        }
    }

    LazyLoader {
        id: screenshotLoader
        active: shell.activeSurface === "screenshot"

        ScreenshotPalette {
            onModeSelected: mode => {
                shell.closeSurface("screenshot");
                Quickshell.execDetached([
                    Quickshell.env("HOME") + "/.local/bin/garage-screenshot-copy",
                    mode
                ]);
            }

            onDismissed: shell.closeSurface("screenshot")
        }
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

        function launcher(): void {
            // Read per call rather than bound: the marker changes underneath a
            // running shell, and an empty one is a session that has not
            // published the setting yet, which means the built-in launcher.
            if (String(launcherMode.text() || "").trim() === "external") {
                shell.closeSurface("launcher");
                return;
            }
            shell.toggleSurface("launcher");
        }

        function closeLauncher(): void {
            shell.closeSurface("launcher");
        }

        function screenshot(): void {
            shell.openSurface("screenshot");
        }

        function closeScreenshot(): void {
            shell.closeSurface("screenshot");
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
