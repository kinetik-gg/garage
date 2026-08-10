import Quickshell
import Quickshell.Bluetooth
import Quickshell.Hyprland
import Quickshell.Networking
import Quickshell.Services.Pipewire
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

// The control centre: the handful of switches that are worth reaching without
// opening System Preferences, plus the volume slider and whatever is playing.
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

    // Grab arming, one turn late: a focus grab installed in the same turn the
    // surface is created is cleared by the click that opened it.
    property bool grabReady: false

    // Suppresses the click-outside dismissal while another shell surface has
    // taken the focus grab on purpose. The screenshot pill is the only thing
    // that sets it: photographing this panel is the whole point of pressing the
    // screenshot bind with it open, so the pill taking the grab must not be read
    // as the user clicking somewhere else. See the grab at the foot of the file.
    property bool holdOpen: false

    readonly property int contentMargin: 12

    // Caffeine is held by this surface, so it lasts as long as the panel does.
    // A hold that outlives the panel needs an inhibitor in the shell rather than
    // in a transient palette; see the note on the inhibitor below.
    property bool caffeine: false

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
    // every keystroke in the session while it is up. The focus grab below is
    // what hands it the keyboard -- and therefore Escape -- when it opens.
    focusable: true
    aboveWindows: true
    exclusiveZone: 0
    surfaceFormat.opaque: false

    // Top and right only: the panel is as tall as its contents, so anchoring the
    // bottom as well would stretch it down the screen.
    anchors {
        top: true
        right: true
    }

    // Overlay surfaces already begin below Waybar's exclusive zone, so the top
    // gutter is measured from there rather than from the top of the screen.
    margins.top: Theme.windowGutter
    margins.right: Theme.windowGutter

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "garage-control-center"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    Component.onCompleted: Qt.callLater(() => centre.grabReady = true)

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

    // Held by this window, so the compositor drops the inhibitor when the panel
    // closes. That also means Caffeine only lasts while the panel is open --
    // making it persist needs the inhibitor to live in the shell rather than in
    // a surface that is destroyed on dismissal.
    IdleInhibitor {
        enabled: centre.caffeine
        window: centre
    }

    ContinuousRectangle {
        id: panel
        anchors.fill: parent
        radius: Theme.cornerRadius
        power: Theme.cornerPower
        // Transparent under glass: garage-control-center is one of the
        // compositor's glass layer namespaces, so the material is drawn beneath
        // this surface and painting a body here would cover it.
        color: Theme.panel
        borderWidth: 1
        borderColor: Theme.frameOuter

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

        // Nothing under the panel should receive the clicks that land on it: the
        // focus grab reports a click outside as a dismissal, and without this the
        // gaps between the tiles would read as outside.
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
                    onToggled: centre.caffeine = !centre.caffeine
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

            // The last thing in the panel. Lock, Sleep and Log Out used to sit
            // below it, and System Preferences below them: the session menu owns
            // the session commands and its own Preferences item, so both were a
            // second copy of a menu one click away -- and the copy was the one
            // that could log the user out from under a mis-aimed click on a
            // volume slider.
            MediaCard {
                Layout.fillWidth: true
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

    // Click anywhere else dismisses, and the grab is also what gives an
    // on-demand layer surface the keyboard when it opens -- without it the
    // Escape below would not be heard until the panel had been clicked.
    HyprlandFocusGrab {
        id: grab
        active: centre.grabReady
        windows: [centre]
        onCleared: {
            if (!centre.grabReady)
                return;
            // Hyprland keeps one grab at a time, so a surface that takes one
            // clears this one whether or not the user clicked anywhere. While
            // the screenshot pill holds it, letting go is not the same as being
            // dismissed: the panel stays up and takes its grab back below.
            if (centre.holdOpen)
                return;
            centre.dismissed();
        }
    }

    // Re-armed by hand rather than by the binding above: a cleared grab is a
    // write to active from the compositor's side, and a written property has no
    // binding left to re-evaluate. One turn late for the same reason it is armed
    // late -- the click that dismissed the pill would otherwise clear the fresh
    // grab and take this panel with it.
    onHoldOpenChanged: {
        if (centre.holdOpen || !centre.grabReady)
            return;
        Qt.callLater(() => {
            grab.active = false;
            grab.active = true;
        });
    }

    Shortcut {
        sequence: "Escape"
        onActivated: centre.dismissed()
    }
}
