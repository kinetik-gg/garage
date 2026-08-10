import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import QtQuick

ShellRoot {
    id: shell
    property string sessionScreenName: ""
    property string preferencesScreenName: ""
    property string preferencesSection: "general"

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
            onMoreInfoRequested: {
                shell.preferencesScreenName = shell.sessionScreenName;
                shell.preferencesSection = "about";
                preferencesLoader.active = true;
            }
        }
    }

    LazyLoader {
        id: launcherLoader
        active: false

        LauncherPalette {
            onDismissed: launcherLoader.active = false
        }
    }

    LazyLoader {
        id: screenshotLoader
        active: false

        ScreenshotPalette {
            onModeSelected: mode => {
                screenshotLoader.active = false;
                Quickshell.execDetached([
                    Quickshell.env("HOME") + "/.local/bin/garage-screenshot-copy",
                    mode
                ]);
            }

            onDismissed: screenshotLoader.active = false
        }
    }

    LazyLoader {
        id: sessionLoader
        active: false

        SessionPalette {
            targetScreenName: shell.sessionScreenName

            onActionSelected: action => {
                sessionLoader.active = false;

                if (action === "about") {
                    aboutLoader.active = true;
                    return;
                }

                if (action === "preferences") {
                    shell.preferencesScreenName = shell.sessionScreenName;
                    shell.preferencesSection = "general";
                    preferencesLoader.active = true;
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

            onDismissed: sessionLoader.active = false
        }
    }

    // The notification service, and the only surface in the shell that is not
    // loaded on demand. Everything else here is opened by the user, so it can wait
    // until they ask; a notification server has to be on the bus before anything
    // sends to it, and nothing in a LazyLoader exists until something activates it.
    // Reaching NotificationDaemon from here is also what brings the singleton --
    // and with it org.freedesktop.Notifications -- up with the shell.
    NotificationPopups {}

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

        function launcher(): void {
            // Read per call rather than bound: the marker changes underneath a
            // running shell, and an empty one is a session that has not
            // published the setting yet, which means the built-in launcher.
            if (String(launcherMode.text() || "").trim() === "external") {
                launcherLoader.active = false;
                return;
            }
            launcherLoader.active = !launcherLoader.active;
        }

        function closeLauncher(): void {
            launcherLoader.active = false;
        }

        function screenshot(): void {
            screenshotLoader.active = true;
        }

        function closeScreenshot(): void {
            screenshotLoader.active = false;
        }

        function session(): void {
            shell.sessionScreenName = Hyprland.focusedMonitor
                ? Hyprland.focusedMonitor.name : "";
            screenshotLoader.active = false;
            sessionLoader.active = !sessionLoader.active;
        }

        function sessionOn(screenName: string): void {
            const sameScreen = shell.sessionScreenName === screenName;
            shell.sessionScreenName = screenName;
            screenshotLoader.active = false;
            sessionLoader.active = sameScreen ? !sessionLoader.active : true;
        }

        function closeSession(): void {
            sessionLoader.active = false;
        }

        function closeAbout(): void {
            aboutLoader.active = false;
        }

        function preferences(): void {
            shell.preferencesScreenName = Hyprland.focusedMonitor
                ? Hyprland.focusedMonitor.name : "";
            shell.preferencesSection = "general";
            preferencesLoader.active = !preferencesLoader.active;
        }

        function preferencesOn(section: string): void {
            shell.preferencesScreenName = Hyprland.focusedMonitor
                ? Hyprland.focusedMonitor.name : "";
            shell.preferencesSection = section;
            preferencesLoader.active = true;
        }

        function closePreferences(): void {
            preferencesLoader.active = false;
        }

        function about(): void {
            shell.sessionScreenName = Hyprland.focusedMonitor
                ? Hyprland.focusedMonitor.name : "";
            aboutLoader.active = !aboutLoader.active;
        }

        function aboutOn(screenName: string): void {
            shell.sessionScreenName = screenName;
            screenshotLoader.active = false;
            sessionLoader.active = false;
            aboutLoader.active = true;
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
