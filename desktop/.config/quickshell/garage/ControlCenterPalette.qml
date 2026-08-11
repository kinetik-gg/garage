import Quickshell
import Quickshell.Bluetooth
import Quickshell.Networking
import Quickshell.Services.Pipewire
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

// The control centre: the handful of switches that are worth reaching without
// opening System Preferences, plus the volume slider. Whatever is playing used
// to sit at the foot of it as a MediaCard; MediaPalette, which the bar's own
// media module opens, is the richer version of the same Mpris logic and owns
// that now, so the copy here is gone rather than kept in step with it.
//
// A layer surface anchored under the bar on the right, the same idiom as the
// notification centre next to it: a panel the next click outside dismisses,
// above fullscreen clients so it stays reachable. Unlike the centre it is not
// full height -- there is a fixed amount in it, so the window wraps its content
// and the glass is only as tall as what it holds.
PanelWindow {
    id: centre

    required property string targetScreenName

    signal dismissed()

    readonly property int contentMargin: 12

    // Caffeine is the shell's, not this panel's. The inhibitor has to outlive
    // the surface that toggles it -- a hold that ends when the control centre is
    // dismissed is a hold nobody asked for -- so the state is owned by shell.qml
    // and this is the input half of it.
    property bool caffeine: false
    signal caffeineToggled()

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            const candidate = Quickshell.screens[index];
            if (candidate.name === centre.targetScreenName)
                return candidate;
        }
        return Quickshell.screens.length > 0 ? Quickshell.screens[0] : null;
    }

    // -- Wi-Fi ---------------------------------------------------------------
    // The first Wi-Fi radio, the same choice the Network pane makes: a second
    // adapter is rare enough that ignoring it beats a device picker in a tile.
    readonly property bool networkAvailable: Networking.backend !== NetworkBackendType.None
    readonly property var wifiDevice: {
        const devices = Networking.devices ? Networking.devices.values : [];
        return devices.find(item => item.type === DeviceType.Wifi) || null;
    }

    // The access point the radio is on. Read from the device's own list without
    // ever touching scannerEnabled: scanning keeps the radio awake, and the
    // connected AP is reported whether or not anything is scanning. Opening this
    // panel must not start a scan the Network pane would then have to stop.
    readonly property var connectedNetwork: {
        const model = centre.wifiDevice ? centre.wifiDevice.networks : null;
        const list = model ? model.values : [];
        return list.find(item => item.connected) || null;
    }

    readonly property bool wifiUsable: centre.networkAvailable
        && centre.wifiDevice !== null && Networking.wifiHardwareEnabled

    readonly property string wifiSubtitle: {
        if (!centre.networkAvailable || centre.wifiDevice === null)
            return "Unavailable";
        if (!Networking.wifiHardwareEnabled)
            return "Blocked";
        if (!Networking.wifiEnabled)
            return "Off";
        if (centre.connectedNetwork === null)
            return "Not Connected";
        const name = String(centre.connectedNetwork.name || "").trim();
        return name !== "" ? name : "Hidden network";
    }

    // -- Bluetooth -----------------------------------------------------------
    readonly property var adapter: Bluetooth.defaultAdapter
    readonly property int bluetoothConnected: {
        const devices = Bluetooth.devices ? Bluetooth.devices.values : [];
        return devices.filter(item => item.connected).length;
    }

    readonly property string bluetoothSubtitle: {
        if (centre.adapter === null)
            return "Unavailable";
        if (!centre.adapter.enabled)
            return "Off";
        return centre.bluetoothConnected > 0
            ? centre.bluetoothConnected + " connected" : "On";
    }

    // -- Appearance and Night Shift ------------------------------------------
    readonly property string themeMode:
        String(controller.preference("appearance", "theme_mode", "dark"))
    readonly property bool nightShift:
        controller.preference("appearance", "night_shift_enabled", true)
    readonly property string nightShiftSchedule:
        String(controller.preference("appearance", "night_shift_start", "18:00"))
        + "–" + String(controller.preference("appearance", "night_shift_end", "06:00"))

    // The opposite of what is *on screen*, which is what Theme.dark reports
    // after the backend has resolved light/dark/auto. Writing the opposite of
    // the configured mode instead would make the tile a no-op from "auto"
    // whenever the schedule already had it on the mode being asked for, and the
    // tile's only promise is that tapping it changes the appearance now.
    function flipAppearance() {
        controller.setPreference("appearance", "theme_mode",
            Theme.dark ? "light" : "dark");
    }

    // -- Volume --------------------------------------------------------------
    // The default sink's audio interface. Nothing on a Pipewire node binds
    // until something tracks it, so the tracker below is what makes these read
    // as anything other than zero.
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var sinkAudio: centre.sink && centre.sink.ready
        ? centre.sink.audio : null
    readonly property real volume: centre.sinkAudio ? centre.sinkAudio.volume : 0
    readonly property bool muted: centre.sinkAudio !== null && centre.sinkAudio.muted

    // Clamped here rather than at each call site: the wheel and the drag both
    // arrive through this, and Pipewire will happily take a volume above 1.
    function applyVolume(value) {
        if (centre.sinkAudio)
            centre.sinkAudio.volume = Math.max(0, Math.min(1, value));
    }

    function toggleMuted() {
        if (centre.sinkAudio)
            centre.sinkAudio.muted = !centre.sinkAudio.muted;
    }

    screen: centre.targetScreen()
    implicitWidth: 390
    // Sized to its content, the same way the toast stack is, and clamped to 1 px
    // for the same reason: a layer surface with no height at all is not a surface
    // the compositor can show, and the column's implicit height is zero for the
    // frame before its children have been laid out.
    implicitHeight: Math.max(1, body.implicitHeight + centre.contentMargin * 2)
    color: "transparent"
    // OnDemand rather than Exclusive, as with the notification centre: this is a
    // panel the user clicks into, not a modal, and an exclusive surface takes
    // every keystroke in the session while it is up. The compositor hands an
    // on-demand layer surface the keyboard as it maps, which is what makes the
    // Escape at the foot of this file heard without a click first.
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    // Top and right only: the panel is as tall as its contents, so anchoring the
    // bottom as well would stretch it down the screen.
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
    }

    // Overlay surfaces already begin below Waybar's exclusive zone, so the top
    // gutter is measured from there rather than from the top of the screen.
    margins.top: motion.surfaceTop
    margins.right: Theme.windowGutter

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "garage-control-center"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    // The preferences backend, scoped to this panel the way PreferencesPalette
    // scopes its own: the tiles that write theme_mode and night_shift_enabled go
    // through the same helper and the same optimistic snapshot as the panes, so
    // a tile and a pane cannot disagree about what is set.
    PreferencesController { id: controller }

    // Mandatory for anything on a PwNode to bind at all. Bound to the current
    // default sink rather than a fixed node, so switching outputs keeps the
    // slider live.
    PwObjectTracker {
        objects: centre.sink ? [centre.sink] : []
    }

    ContinuousRectangle {
        id: panel
        opacity: motion.opacity
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass: garage-control-center is one of the
        // compositor's glass layer namespaces, so the material is drawn beneath
        // this surface and painting a body here would cover it.
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

        // The panel eats the clicks that land in the gaps between its tiles
        // rather than leaving them unhandled.
        MouseArea {
            anchors.fill: parent
        }

        // Anchored rather than filled: the window's height is this column's
        // implicit height, so the column must not be told how tall it is.
        ColumnLayout {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: centre.contentMargin
            spacing: 10

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 8
                rowSpacing: 8

                ControlTile {
                    Layout.fillWidth: true
                    iconSource: "icons/wifi-high.svg"
                    title: "Wi-Fi"
                    subtitle: centre.wifiSubtitle
                    active: Networking.wifiEnabled
                    enabled: centre.wifiUsable
                    onToggled: Networking.wifiEnabled = !Networking.wifiEnabled
                }

                ControlTile {
                    Layout.fillWidth: true
                    iconSource: "icons/bluetooth.svg"
                    title: "Bluetooth"
                    subtitle: centre.bluetoothSubtitle
                    active: centre.adapter !== null && centre.adapter.enabled
                    enabled: centre.adapter !== null
                    onToggled: centre.adapter.enabled = !centre.adapter.enabled
                }

                ControlTile {
                    Layout.fillWidth: true
                    iconSource: "icons/moon.svg"
                    title: "Do Not Disturb"
                    subtitle: NotificationDaemon.dnd ? "Notifications silenced" : "Off"
                    active: NotificationDaemon.dnd
                    // toggleDnd rather than the alias: do-not-disturb is
                    // persisted, and every write has to reach the file.
                    onToggled: NotificationDaemon.toggleDnd()
                }

                ControlTile {
                    Layout.fillWidth: true
                    iconSource: "icons/sun.svg"
                    title: "Night Shift"
                    subtitle: centre.nightShift ? centre.nightShiftSchedule : "Off"
                    active: centre.nightShift
                    onToggled: controller.setPreference("appearance",
                        "night_shift_enabled", !centre.nightShift)
                }

                ControlTile {
                    Layout.fillWidth: true
                    iconSource: "icons/coffee.svg"
                    title: "Caffeine"
                    subtitle: centre.caffeine ? "Display stays awake" : "Off"
                    active: centre.caffeine
                    onToggled: centre.caffeineToggled()
                }

                ControlTile {
                    Layout.fillWidth: true
                    iconSource: "icons/palette.svg"
                    title: "Appearance"
                    subtitle: centre.themeMode === "auto"
                        ? "Auto" : (Theme.dark ? "Dark" : "Light")
                    active: Theme.dark
                    onToggled: centre.flipAppearance()
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 2
                spacing: 8
                enabled: centre.sinkAudio !== null
                opacity: centre.sinkAudio !== null ? 1 : 0.45

                // The speaker is the mute button, the way the moon in the
                // notification centre's header is the label for its switch.
                ContinuousRectangle {
                    Layout.preferredWidth: 28
                    Layout.preferredHeight: 28
                    Layout.alignment: Qt.AlignVCenter
                    radius: Theme.controlRadius
                    color: mutePointer.containsMouse ? Theme.hoverStrong : "transparent"

                    Image {
                        id: speakerGlyph
                        anchors.centerIn: parent
                        width: 17
                        height: 17
                        source: centre.muted
                            ? "icons/speaker-slash.svg" : "icons/speaker-high.svg"
                        sourceSize.width: 34
                        sourceSize.height: 34
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        antialiasing: true
                        mipmap: true
                        visible: false
                    }

                    ColorOverlay {
                        anchors.fill: speakerGlyph
                        source: speakerGlyph
                        color: centre.muted ? Theme.textMuted : Theme.text
                        cached: true
                    }

                    MouseArea {
                        id: mutePointer
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: centre.toggleMuted()
                    }
                }

                SettingsSlider {
                    id: volumeSlider
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignVCenter
                    value: centre.volume
                    // The live value, not the committed one: the volume is meant
                    // to follow the handle. Scrolling reports through moved() as
                    // well, so the wheel needs nothing of its own here.
                    onMoved: centre.applyVolume(volumeSlider.value)
                }

                Text {
                    Layout.preferredWidth: 34
                    Layout.alignment: Qt.AlignVCenter
                    text: Math.round(centre.volume * 100) + "%"
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 11
                    horizontalAlignment: Text.AlignRight
                    renderType: Text.NativeRendering
                }
            }
        }
    }

    // A slider writes its own value as it is dragged or scrolled, which destroys
    // the declarative binding above the first time it is touched. Syncing from here
    // keeps the slider following the sink afterwards -- pactl, a headset key,
    // another shell surface -- instead of freezing on whatever the last gesture
    // left. Not while the handle is held, so a drag is not fought by the round
    // trip.
    Connections {
        target: centre
        function onVolumeChanged() {
            if (!volumeSlider.pressed)
                volumeSlider.value = centre.volume;
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
