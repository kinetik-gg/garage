import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland
import QtQuick

ShellRoot {
    id: shell
    // Timers and the stopwatch outlive the lazy launcher surface that controls
    // them. Holding the singleton at shell scope keeps its deadline checks and
    // persistence active after the palette closes.
    readonly property var timerService: TimerService
    property string sessionScreenName: ""
    property string sessionInitialAction: ""
    property string preferencesScreenName: ""
    property string preferencesSection: "general"
    property string notificationScreenName: ""
    property string controlCenterScreenName: ""
    property string launcherScreenName: ""
    property string monitorScreenName: ""
    // Monitor-local X coordinate of the complete Waybar metrics group's centre.
    // A negative value means a keybind opened the dashboard, so it uses screen centre.
    property real monitorAnchorX: -1
    property string mediaScreenName: ""
    // Monitor-local click position for the variable-width media label. A keybind
    // has no bar click and leaves this negative for output-centred placement.
    property real mediaAnchorX: -1
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

    // Caffeine, held here rather than in the control centre that toggles it.
    //
    // An idle inhibitor is a property of a Wayland surface, and the control
    // centre is destroyed on dismissal -- so an inhibitor living there was
    // dropped by the compositor the moment the panel closed, which is to say
    // the setting did nothing at all except while its own switch was on screen.
    // The surface below exists for no other reason than to be something the
    // inhibitor can be attached to, and only while the setting is on.
    property bool caffeine: false

    LazyLoader {
        active: shell.caffeine

        PanelWindow {
            id: caffeineHold
            // One pixel, on the background layer, drawing nothing and masked out
            // of input entirely: this is a handle for the inhibitor, not
            // something anyone should be able to see or click.
            implicitWidth: 1
            implicitHeight: 1
            color: "transparent"
            // A transparent colour on a surface the compositor was handed as
            // opaque composites as black, so without this the handle is a black
            // pixel on the wallpaper for as long as Caffeine is on.
            surfaceFormat.opaque: false
            exclusiveZone: 0
            focusable: false
            WlrLayershell.layer: WlrLayer.Background
            WlrLayershell.namespace: "garage-caffeine"
            mask: Region {}

            IdleInhibitor {
                enabled: true
                window: caffeineHold
            }
        }
    }

    function focusedScreenName() {
        return Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : "";
    }

    // The surfaces the pill may share the screen with: the panels worth
    // taking a picture of. All of them stay up through the capture, which is
    // what disarming the dismiss catcher below is for.
    function sharesScreenWithScreenshot(name) {
        return name === "notificationCenter" || name === "controlCenter"
            || name === "monitorPalette" || name === "mediaPalette"
            || name === "aiPalette" || name === "launcher";
    }

    // The surfaces whose click-outside dismissal the shared catcher owns. The
    // session menu is among them now: its separate mouse-bind dismissal died
    // with the bar swap, and the catcher's geometry trick -- collapse to 1px
    // rather than unmap when disarmed -- is what replaces it.
    function catchesOutsideClicks(name) {
        return name === "launcher"
            || name === "notificationCenter"
            || name === "controlCenter"
            || name === "session"
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
            // where it is rather than being taken down with the panel. The
            // monitor dashboard keeps its loader alive for the length of its
            // exit; every other surface goes down immediately.
            shell.requestCloseSurface(name);
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

    // The loaders whose surface plays an exit before it may be destroyed. Each
    // of these owns a PanelMotion and raises dismissed() when it has finished;
    // clearing activeSurface here instead would destroy the window mid-fade.
    function animatedLoader(name) {
        if (name === "monitorPalette")      return monitorPaletteLoader;
        if (name === "controlCenter")       return controlCenterLoader;
        if (name === "notificationCenter")  return notificationCenterLoader;
        if (name === "mediaPalette")        return mediaPaletteLoader;
        if (name === "aiPalette")           return aiPaletteLoader;
        if (name === "session")             return sessionLoader;
        if (name === "launcher")            return launcherLoader;
        return null;
    }

    // Closing, but through the surface itself where the surface has something to
    // play on the way out. Anything not in the list above is closed by clearing
    // activeSurface, which destroys its loader on the spot.
    function requestCloseSurface(name) {
        const loader = shell.animatedLoader(name);
        if (loader && loader.item && loader.item.requestDismissal) {
            loader.item.requestDismissal();
            return;
        }
        shell.closeSurface(name);
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

    function confirmLauncherSessionAction(action) {
        shell.openSurface("session", () => {
            shell.sessionScreenName = shell.launcherScreenName;
            shell.sessionInitialAction = action;
        });
    }

    function runLauncherShellAction(action) {
        if (action === "settings") {
            shell.openWindow(preferencesLoader, () => {
                shell.preferencesScreenName = shell.launcherScreenName;
                shell.preferencesSection = "general";
            });
            return;
        }
        if (action === "dnd")
            NotificationDaemon.toggleDnd();
        else if (action === "caffeine")
            shell.caffeine = !shell.caffeine;
        else if (action === "night")
            Quickshell.execDetached([Quickshell.env("HOME") + "/.local/bin/garage",
                "action", "appearance.night_shift.toggle"]);
        else if (action === "theme")
            Quickshell.execDetached([Quickshell.env("HOME") + "/.local/bin/garage",
                "set", "appearance.theme_mode", JSON.stringify(Theme.dark ? "light" : "dark")]);
        shell.closeSurface("launcher");
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
        onDismissed: shell.requestCloseSurface(shell.activeSurface)
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
            caffeine: shell.caffeine
            onSessionActionRequested: action => shell.confirmLauncherSessionAction(action)
            onShellActionRequested: action => shell.runLauncherShellAction(action)
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

    // The wedge guard on that lifetime.
    //
    // Measured on this machine: region capture was opened, slurp sat waiting for
    // a drag that never came, and it went unnoticed for over six minutes. The
    // catcher's `armed` expression above stands down for the whole time
    // captureProcess is running -- which is correct, slurp's own clicks are not
    // the user clicking away -- so for those six minutes click-outside dismissal
    // was dead shell-wide: the launcher, both centres and all three bar panels
    // could only be closed with Escape, and nothing on screen said why. One
    // forgotten selection disables a gesture in six unrelated surfaces.
    //
    // So the stand-down is given a ceiling rather than being made conditional:
    // `armed` is untouched, and this puts a bound on how long it can be false.
    // Two minutes is far past any real region drag and far short of the six
    // minutes that were measured.
    //
    // SIGTERM through Process's own signal(), then `running = false`, which is
    // Quickshell's terminate-then-kill path for anything that ignores it and is
    // also the property `armed` reads -- so clearing it is what actually gives
    // the shell its click-outside gesture back.
    //
    // What this does not promise: signal() reaches one pid, and the command is
    // the garage-screenshot-copy wrapper, which runs slurp as a foreground child
    // in a command substitution. bash defers a signal until that child returns,
    // so a slurp already sitting there may outlive the wrapper as an orphan and
    // still have to be dismissed with Escape. That is the tool being ignored; the
    // shell-wide dead dismissal is the part that was doing damage to five other
    // surfaces, and this ends it. Reaching the child as well means the wrapper
    // trapping and forwarding, which is that script's job, not this file's.
    Timer {
        id: captureWedgeGuard
        interval: 120000
        repeat: false
        running: captureProcess.running
        onTriggered: {
            if (!captureProcess.running)
                return;
            captureProcess.signal(15);
            captureProcess.running = false;
        }
    }

    LazyLoader {
        id: sessionLoader
        active: shell.activeSurface === "session"

        SessionPalette {
            targetScreenName: shell.sessionScreenName
            initialAction: shell.sessionInitialAction

            onActionSelected: action => {
                shell.sessionInitialAction = "";
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

            onDismissed: {
                shell.sessionInitialAction = "";
                shell.closeSurface("session");
            }
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
            caffeine: shell.caffeine
            onCaffeineToggled: shell.caffeine = !shell.caffeine
            onDismissed: shell.closeSurface("controlCenter")
        }
    }

    // The three panels the bar's own modules open, on the same shape as the two
    // centres above: one loader each, bound to activeSurface, and a dismissed()
    // that clears the name it was activated by. AI usage sits against the right
    // edge. Media and monitor receive a monitor-local X from the bar click and
    // centre themselves under it, clamped inside the output.
    LazyLoader {
        id: monitorPaletteLoader
        active: shell.activeSurface === "monitorPalette"

        MonitorPalette {
            targetScreenName: shell.monitorScreenName
            targetAnchorX: shell.monitorAnchorX
            onDismissed: shell.closeSurface("monitorPalette")
        }
    }

    LazyLoader {
        id: mediaPaletteLoader
        active: shell.activeSurface === "mediaPalette"

        MediaPalette {
            targetScreenName: shell.mediaScreenName
            targetAnchorX: shell.mediaAnchorX
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

    // The bar. Its module clicks arrive here with the screen they were clicked on
    // and, where the old waybar click carried one, the anchor X under the module --
    // routed through the very same functions the keybinds use, so a bar click and a
    // keybind can never disagree about what opens.
    Bar {
        onSurfaceRequested: (surface, screenName, anchorX) => {
            if (surface === "session")
                shell.sessionOn(screenName);
            else if (surface === "launcher")
                shell.launcherOn(screenName);
            else if (surface === "notifications")
                shell.notificationsOn(screenName);
            else if (surface === "controlCenter")
                shell.controlCenterOn(screenName);
            else if (surface === "media")
                shell.mediaOn(screenName, anchorX);
            else if (surface === "aiUsage")
                shell.aiUsageOn(screenName);
            else if (surface.startsWith("monitor:"))
                shell.monitorOn(screenName, anchorX);
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
                shell.requestCloseSurface("launcher");
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

        function launcherOn(screenName: string): void {
            if (String(launcherMode.text() || "").trim() === "external") {
                shell.requestCloseSurface("launcher");
                return;
            }
            const sameScreen = shell.launcherScreenName === screenName;
            const open = () => {
                shell.launcherScreenName = screenName;
            };
            if (sameScreen)
                shell.toggleSurface("launcher", open);
            else
                shell.openSurface("launcher", open);
        }

        function closeLauncher(): void {
            shell.requestCloseSurface("launcher");
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
                shell.sessionInitialAction = "";
            });
        }

        function sessionOn(screenName: string): void {
            // Asked for again on the screen it is already on, it closes; asked
            // for on another screen, it moves there rather than closing under a
            // pointer that is somewhere else entirely.
            const sameScreen = shell.sessionScreenName === screenName;
            const open = () => {
                shell.sessionScreenName = screenName;
                shell.sessionInitialAction = "";
            };
            if (sameScreen)
                shell.toggleSurface("session", open);
            else
                shell.openSurface("session", open);
        }

        // Through the surface, like every other close*: this is the session
        // menu's only click-outside path -- garage-menu-dismiss calls it -- so
        // clearing activeSurface here would give the menu an exit on Escape and
        // none on the gesture it is actually dismissed with.
        function closeSession(): void {
            shell.requestCloseSurface("session");
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
            shell.requestCloseSurface("notificationCenter");
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
            shell.requestCloseSurface("controlCenter");
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
                shell.monitorAnchorX = -1;
            });
        }

        function monitorOn(screenName: string, anchorX: int): void {
            const sameScreen = shell.monitorScreenName === screenName;
            const open = () => {
                shell.monitorScreenName = screenName;
                shell.monitorAnchorX = anchorX;
            };
            if (sameScreen)
                shell.toggleSurface("monitorPalette", open);
            else
                shell.openSurface("monitorPalette", open);
        }

        function closeMonitor(): void {
            shell.requestCloseSurface("monitorPalette");
        }

        function media(): void {
            shell.toggleSurface("mediaPalette", () => {
                shell.mediaScreenName = shell.focusedScreenName();
                shell.mediaAnchorX = -1;
            });
        }

        function mediaOn(screenName: string, anchorX: int): void {
            const sameScreen = shell.mediaScreenName === screenName;
            const open = () => {
                shell.mediaScreenName = screenName;
                shell.mediaAnchorX = anchorX;
            };
            if (sameScreen)
                shell.toggleSurface("mediaPalette", open);
            else
                shell.openSurface("mediaPalette", open);
        }

        function closeMedia(): void {
            shell.requestCloseSurface("mediaPalette");
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
            shell.requestCloseSurface("aiPalette");
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
