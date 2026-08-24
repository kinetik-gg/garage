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
    property real sessionAnchor: -1
    property string sessionInitialAction: ""
    property string preferencesScreenName: ""
    property string preferencesSection: "general"
    property string notificationScreenName: ""
    property real notificationAnchor: -1
    property string controlCenterScreenName: ""
    property real controlCenterAnchor: -1
    property string launcherScreenName: ""
    property real launcherAnchor: -1
    property string launcherInitialMode: "default"
    property string monitorScreenName: ""
    // Monitor-local coordinate of the system widget's centre.
    // A negative value means a keybind opened the dashboard, so it uses screen centre.
    property real monitorAnchorX: -1
    property string mediaScreenName: ""
    // Monitor-local click position for the variable-width media label. A keybind
    // has no bar click and leaves this negative for output-centred placement.
    property real mediaAnchorX: -1
    property string aiUsageScreenName: ""
    property real aiUsageAnchor: -1

    // Declarative routing table for every transient panel. The compatibility
    // function names below are deliberately thin shims over this table so IPC
    // callers and bar widgets cannot drift into two sets of behavior.
    readonly property var surfaces: ({
        session: {
            panel: sessionLoader, screen: () => shell.sessionScreenName,
            setScreen: value => shell.sessionScreenName = value,
            setAnchor: value => shell.sessionAnchor = value,
            anchor: "axis", catches: false, screenshot: false, animated: true
        },
        launcher: {
            panel: launcherLoader, screen: () => shell.launcherScreenName,
            setScreen: value => shell.launcherScreenName = value,
            setAnchor: value => shell.launcherAnchor = value,
            anchor: "axis", catches: true, screenshot: true, animated: true
        },
        notifications: {
            panel: notificationCenterLoader,
            screen: () => shell.notificationScreenName,
            setScreen: value => shell.notificationScreenName = value,
            setAnchor: value => shell.notificationAnchor = value,
            anchor: "axis", catches: true, screenshot: true, animated: true
        },
        "control-center": {
            panel: controlCenterLoader,
            screen: () => shell.controlCenterScreenName,
            setScreen: value => shell.controlCenterScreenName = value,
            setAnchor: value => shell.controlCenterAnchor = value,
            anchor: "axis", catches: true, screenshot: true, animated: true
        },
        system: {
            panel: monitorPaletteLoader, screen: () => shell.monitorScreenName,
            setScreen: value => shell.monitorScreenName = value,
            setAnchor: value => shell.monitorAnchorX = value,
            anchor: "axis", catches: true, screenshot: true, animated: true
        },
        media: {
            panel: mediaPaletteLoader, screen: () => shell.mediaScreenName,
            setScreen: value => shell.mediaScreenName = value,
            setAnchor: value => shell.mediaAnchorX = value,
            anchor: "axis", catches: true, screenshot: true, animated: true
        },
        "ai-usage": {
            panel: aiUsagePaletteLoader, screen: () => shell.aiUsageScreenName,
            setScreen: value => shell.aiUsageScreenName = value,
            setAnchor: value => shell.aiUsageAnchor = value,
            anchor: "axis", catches: true, screenshot: true, animated: true
        }
    })

    // Which transient surface is on screen, by name, or "" for none.
    //
    // The transient surfaces -- the launcher, the session menu, the notification
    // centre, the control centre and the three bar-detail panels (system,
    // now-playing, and AI usage) -- are
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

    function canonicalSurface(name) {
        const aliases = {
            notificationCenter: "notifications",
            controlCenter: "control-center",
            monitorPalette: "system",
            mediaPalette: "media",
            aiPalette: "ai-usage",
            aiUsage: "ai-usage"
        };
        if (String(name).startsWith("monitor:"))
            return "system";
        return aliases[name] || name;
    }

    function surfaceSpec(name) {
        return surfaces[canonicalSurface(name)] || null;
    }

    // The surfaces the pill may share the screen with: the panels worth
    // taking a picture of. All of them stay up through the capture, which is
    // what disarming the dismiss catcher below is for.
    function sharesScreenWithScreenshot(name) {
        const spec = surfaceSpec(name);
        return spec !== null && spec.screenshot;
    }

    // The surfaces whose click-outside dismissal the shared catcher owns.
    // Not the session menu: it carries a fullscreen backdrop of its own, the
    // same pattern its confirmation dialog uses, and dismisses through it.
    function catchesOutsideClicks(name) {
        const spec = surfaceSpec(name);
        return spec !== null && spec.catches;
    }

    // Whether a palette that is up has asked not to be dismissed out from under
    // a gesture still in progress.
    //
    // MonitorPalette and MediaPalette each carry a holdOpen for this, and each
    // documents the shell binding the shared catcher's `armed` to it: the live
    // case is MediaPalette's seek bar, where the pointer leaves the panel during
    // the drag and the release is the tail of that drag rather than the user
    // clicking away. Read through the loaders' items because the palettes only
    // exist while they are up.
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
        name = canonicalSurface(name);
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
        name = canonicalSurface(name);
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
        name = canonicalSurface(name);
        if (shell.activeSurface === name)
            shell.activeSurface = "";
    }

    // The loaders whose surface plays an exit before it may be destroyed. Each
    // of these owns a PanelMotion and raises dismissed() when it has finished;
    // clearing activeSurface here instead would destroy the window mid-fade.
    function animatedLoader(name) {
        const spec = surfaceSpec(name);
        return spec && spec.animated ? spec.panel : null;
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
            GaragePaths.screenshotCopy,
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
            shell.sessionAnchor = shell.launcherAnchor;
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
            Quickshell.execDetached([GaragePaths.garage,
                "action", "appearance.night_shift.toggle"]);
        else if (action === "theme")
            Quickshell.execDetached([GaragePaths.garage,
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

    // The surface openers live on the shell itself so the bar's in-process
    // router and the IPC handler reach the same bodies: one function per
    // surface, however it is invoked.

    function surfaceOn(name, screenName, anchor) {
        const canonical = canonicalSurface(name);
        const spec = surfaceSpec(canonical);
        if (!spec)
            return;
        if (canonical === "launcher"
                && String(launcherMode.text() || "").trim() === "external") {
            shell.requestCloseSurface("launcher");
            return;
        }
        const sameScreen = spec.screen() === screenName;
        const configure = () => {
            if (canonical === "launcher")
                shell.launcherInitialMode = "default";
            spec.setScreen(screenName);
            if (spec.setAnchor)
                spec.setAnchor(anchor === undefined ? -1 : anchor);
            if (canonical === "session")
                shell.sessionInitialAction = "";
        };
        if (sameScreen)
            shell.toggleSurface(canonical, configure);
        else
            shell.openSurface(canonical, configure);
    }

    function launcherOn(screenName: string): void {
        shell.surfaceOn("launcher", screenName, -1);
    }
    function launcherClipOn(screenName: string): void {
        if (shell.activeSurface === "launcher"
                && shell.launcherInitialMode === "clip"
                && shell.launcherScreenName === screenName) {
            shell.requestCloseSurface("launcher");
            return;
        }
        shell.launcherInitialMode = "clip";
        shell.openSurface("launcher", () => {
            shell.launcherScreenName = screenName;
            shell.launcherAnchor = -1;
        });
    }
    function sessionOn(screenName: string): void {
        shell.surfaceOn("session", screenName, -1);
    }
    function notificationsOn(screenName: string): void {
        shell.surfaceOn("notifications", screenName, -1);
    }
    function controlCenterOn(screenName: string): void {
        shell.surfaceOn("control-center", screenName, -1);
    }
    function monitorOn(screenName: string, anchorX: int): void {
        shell.systemOn(screenName, anchorX);
    }
    function systemOn(screenName: string, anchor: int): void {
        shell.surfaceOn("system", screenName, anchor);
    }
    function mediaOn(screenName: string, anchorX: int): void {
        shell.surfaceOn("media", screenName, anchorX);
    }
    function aiUsageOn(screenName: string): void {
        shell.surfaceOn("ai-usage", screenName, -1);
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
            initialMode: shell.launcherInitialMode
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
            targetAnchor: shell.sessionAnchor
            edge: BarState.position
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
        active: shell.activeSurface === "notifications"

        NotificationCenterPalette {
            targetScreenName: shell.notificationScreenName
            targetAnchor: shell.notificationAnchor
            edge: BarState.position
            onDismissed: shell.closeSurface("notifications")
        }
    }

    LazyLoader {
        id: controlCenterLoader
        active: shell.activeSurface === "control-center"

        ControlCenterPalette {
            targetScreenName: shell.controlCenterScreenName
            targetAnchor: shell.controlCenterAnchor
            edge: BarState.position
            caffeine: shell.caffeine
            onCaffeineToggled: shell.caffeine = !shell.caffeine
            onDismissed: shell.closeSurface("control-center")
        }
    }

    // The three bar-detail panels, on the same shape as the two
    // centres above: one loader each, bound to activeSurface, and a dismissed()
    // that clears the name it was activated by. Media and system receive a
    // monitor-local coordinate from the bar click and
    // centre themselves under it, clamped inside the output.
    LazyLoader {
        id: monitorPaletteLoader
        active: shell.activeSurface === "system"

        MonitorPalette {
            targetScreenName: shell.monitorScreenName
            targetAnchor: shell.monitorAnchorX
            edge: BarState.position
            onDismissed: shell.closeSurface("system")
        }
    }

    LazyLoader {
        id: mediaPaletteLoader
        active: shell.activeSurface === "media"

        MediaPalette {
            targetScreenName: shell.mediaScreenName
            targetAnchor: shell.mediaAnchorX
            edge: BarState.position
            onDismissed: shell.closeSurface("media")
        }
    }

    LazyLoader {
        id: aiUsagePaletteLoader
        active: shell.activeSurface === "ai-usage"

        ExtensionPopupSurface {
            extensionId: "ai-usage"
            targetScreenName: shell.aiUsageScreenName
            targetAnchor: shell.aiUsageAnchor
            onDismissed: shell.closeSurface("ai-usage")
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
            shell.notificationAnchor = -1;
            shell.openSurface("notifications", () => {});
        }
    }

    // Hardware feedback is shell-owned now; its IPC handler lives with the OSD
    // so the mutation binds do not need to know anything about presentation.
    Osd {}

    // The bar. Its module clicks arrive here with the screen they were clicked on
    // and, where a bar widget has one, its long-axis anchor under the module --
    // routed through the very same functions the keybinds use, so a bar click and a
    // keybind can never disagree about what opens.
    Bar {
        onSurfaceRequested: (surface, screenName, anchorX) => {
            shell.surfaceOn(surface, screenName, anchorX);
        }
    }

    // Whether the shell's own launcher is switched on. SUPER+Space already asks
    // the same marker through its wrapper; this closes the other routes in, so
    // nothing can open a launcher the user turned off.
    FileView {
        id: launcherMode
        path: GaragePaths.launcherMode
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
        property bool aiUsageVisible: aiUsagePaletteLoader.active


        function launcherOn(screenName: string): void {
            shell.launcherOn(screenName);
        }

        function sessionOn(screenName: string): void {
            shell.sessionOn(screenName);
        }

        function notificationsOn(screenName: string): void {
            shell.notificationsOn(screenName);
        }

        function controlCenterOn(screenName: string): void {
            shell.controlCenterOn(screenName);
        }

        function monitorOn(screenName: string, anchorX: int): void {
            shell.monitorOn(screenName, anchorX);
        }

        function systemOn(screenName: string, anchor: int): void {
            shell.systemOn(screenName, anchor);
        }

        function mediaOn(screenName: string, anchorX: int): void {
            shell.mediaOn(screenName, anchorX);
        }

        function aiUsageOn(screenName: string): void {
            shell.aiUsageOn(screenName);
        }

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
            shell.surfaceOn("launcher", shell.focusedScreenName(), -1);
        }

        function launcherClip(): void {
            // Clipboard history is a native source even when Super+Space uses
            // an external application launcher; that external setting has no
            // clipboard-mode contract to delegate to.
            shell.launcherClipOn(shell.focusedScreenName());
        }

        function mediaAction(action: string): void {
            MediaController.dispatch(action);
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
            shell.surfaceOn("session", shell.focusedScreenName(), -1);
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
            shell.surfaceOn("notifications", shell.focusedScreenName(), -1);
        }


        function closeNotifications(): void {
            shell.requestCloseSurface("notifications");
        }

        // The control centre, on the same three-function shape as the centre
        // above: the bar's glyph and SUPER+CTRL+A both come in through
        // garage-panel-toggle, which resolves the monitor under the pointer and
        // calls controlCenterOn with it.
        function controlCenter(): void {
            shell.surfaceOn("control-center", shell.focusedScreenName(), -1);
        }


        function closeControlCenter(): void {
            shell.requestCloseSurface("control-center");
        }

        // The bar panels, each on the same three-function shape as the
        // control centre above. The click on a bar module goes through
        // garage-panel-toggle, which resolves the monitor under the pointer and
        // calls the *On form with it; the parameterless form is there for a
        // keybind, which has no pointer to resolve and so takes the focused
        // monitor, the way launcher() and session() do.
        function monitor(): void {
            shell.surfaceOn("system", shell.focusedScreenName(), -1);
        }

        function system(): void {
            shell.surfaceOn("system", shell.focusedScreenName(), -1);
        }


        function closeMonitor(): void {
            shell.requestCloseSurface("system");
        }

        function closeSystem(): void {
            shell.requestCloseSurface("system");
        }

        function media(): void {
            shell.surfaceOn("media", shell.focusedScreenName(), -1);
        }


        function closeMedia(): void {
            shell.requestCloseSurface("media");
        }

        function aiUsage(): void {
            shell.surfaceOn("ai-usage", shell.focusedScreenName(), -1);
        }


        function closeAiUsage(): void {
            shell.requestCloseSurface("ai-usage");
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
