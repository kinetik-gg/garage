import Quickshell
import Quickshell.Wayland
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

Scope {
    id: preferences
    required property string targetScreenName
    required property string initialSection
    property string currentSection: ""
    onInitialSectionChanged: showSection(initialSection)
    onCurrentSectionChanged: paneLoader.setSource(sourceFor(currentSection), { "controller": controller })

    Component.onCompleted: showSection(initialSection)
    signal dismissed()

    readonly property var categories: [
        { key: "general", title: "General", icon: "icons/sliders.svg", source: "GeneralPane.qml" },
        { key: "appearance", title: "Appearance", icon: "icons/palette.svg", source: "AppearancePane.qml" },
        { key: "wallpaper", title: "Wallpaper", icon: "icons/image.svg", source: "WallpaperPane.qml" },
        { key: "displays", title: "Displays", icon: "icons/monitor.svg", source: "DisplaysPane.qml" },
        { key: "workspaces", title: "Workspaces", icon: "icons/squares-four.svg", source: "WorkspacesPane.qml" },
        { key: "menubar", title: "Menu Bar", icon: "icons/browser.svg", source: "MenuBarPane.qml" },
        { key: "network", title: "Network", icon: "icons/wifi-high.svg", source: "NetworkPane.qml" },
        { key: "bluetooth", title: "Bluetooth", icon: "icons/bluetooth.svg", source: "BluetoothPane.qml" },
        { key: "sound", title: "Sound", icon: "icons/speaker-high.svg", source: "SoundPane.qml" },
        { key: "input", title: "Input", icon: "icons/mouse.svg", source: "InputPane.qml" },
        { key: "keybindings", title: "Keyboard", icon: "icons/keyboard.svg", source: "KeybindingsPane.qml" },
        { key: "notifications", title: "Notifications", icon: "icons/bell.svg", source: "NotificationsPane.qml" },
        { key: "lock", title: "Lock & Idle", icon: "icons/lock-key.svg", source: "LockPane.qml" },
        { key: "language", title: "Language & Region", icon: "icons/globe.svg", source: "LanguagePane.qml" },
        { key: "about", title: "About", icon: "icons/info.svg", source: "AboutPane.qml" }
    ]

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index) {
            if (Quickshell.screens[index].name === targetScreenName)
                return Quickshell.screens[index];
        }
        return Quickshell.screens.length ? Quickshell.screens[0] : null;
    }

    function sourceFor(section) {
        const category = categories.find(item => item.key === section);
        return category ? category.source : "GeneralPane.qml";
    }

    function titleFor(section) {
        const category = categories.find(item => item.key === section);
        return category ? category.title : "System Preferences";
    }

    function showSection(section) {
        if (categories.some(item => item.key === section))
            currentSection = section;
    }

    PreferencesController { id: controller }

    // A real toplevel rather than a layer surface, so the compositor can move,
    // stack and focus it like any other window. It also picks up the window
    // glass decoration instead of the layer one.
    FloatingWindow {
        id: window
        title: "System Preferences"
        implicitWidth: 720
        implicitHeight: 640
        minimumSize: Qt.size(560, 460)
        color: "transparent"

        Rectangle {
            anchors.fill: parent
            color: Theme.panel

            RowLayout {
                anchors.fill: parent
                spacing: 0

                Rectangle {
                    Layout.preferredWidth: 220
                    Layout.fillHeight: true
                    color: Theme.sidebarWell

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 14
                        anchors.rightMargin: 14
                        anchors.topMargin: 22
                        anchors.bottomMargin: 18
                        spacing: 3

                        Repeater {
                            model: preferences.categories.filter(category => category.key !== "about")
                            PreferencesSidebarItem {
                                required property var modelData
                                Layout.fillWidth: true
                                title: modelData.title
                                iconSource: modelData.icon
                                selected: preferences.currentSection === modelData.key
                                onActivated: preferences.showSection(modelData.key)
                            }
                        }

                        Item { Layout.fillHeight: true }

                        PreferencesSidebarItem {
                            Layout.fillWidth: true
                            title: "About"
                            iconSource: "icons/info.svg"
                            selected: preferences.currentSection === "about"
                            onActivated: preferences.showSection("about")
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: Theme.contentTint

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 30
                        anchors.rightMargin: 30
                        anchors.topMargin: 14
                        anchors.bottomMargin: 20
                        spacing: 12

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 38

                            Text {
                                Layout.fillWidth: true
                                text: preferences.titleFor(preferences.currentSection)
                                color: Theme.text
                                font.family: Theme.sans
                                font.pixelSize: 24
                                font.weight: Font.Bold
                                renderType: Text.NativeRendering
                            }

                            ContinuousRectangle {
                                Layout.preferredWidth: 38
                                Layout.preferredHeight: 38
                                radius: Theme.controlRadius
                                color: closePointer.containsMouse ? Theme.hoverStrong : "transparent"

                                Image {
                                    id: closeGlyph
                                    anchors.centerIn: parent
                                    width: 18
                                    height: 18
                                    source: "icons/x.svg"
                                    sourceSize.width: 36
                                    sourceSize.height: 36
                                    smooth: true
                                    antialiasing: true
                                    mipmap: true
                                    visible: false
                                }

                                // The svg ships its own colour, so it needs an
                                // overlay to be recoloured at all.
                                ColorOverlay {
                                    anchors.fill: closeGlyph
                                    source: closeGlyph
                                    color: Theme.text
                                    cached: true
                                }
                                MouseArea {
                                    id: closePointer
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: preferences.dismissed()
                                }
                            }
                        }

                        ContinuousRectangle {
                            Layout.fillWidth: true
                            visible: controller.error !== ""
                            implicitHeight: errorText.implicitHeight + 20
                            color: Theme.hoverStrong
                            borderWidth: 1
                            borderColor: Theme.border

                            Text {
                                id: errorText
                                anchors.fill: parent
                                anchors.margins: 10
                                text: controller.error
                                color: Theme.textMuted
                                font.family: Theme.sans
                                font.pixelSize: 11
                                wrapMode: Text.WordWrap
                                renderType: Text.NativeRendering
                            }
                        }

                        Loader {
                            id: paneLoader
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                        }
                    }

                    WallpaperPicker {
                        controller: controller
                        targetScreenName: preferences.targetScreenName
                        parentWindow: window
                        targetKey: controller.wallpaperPickerKey
                        startFolder: controller.wallpaperPickerFolder
                        pickerVisible: controller.wallpaperPickerOpen
                        onDismissed: controller.wallpaperPickerOpen = false
                    }

                    // Only before the first snapshot. Later refreshes have a
                    // populated pane behind them, and covering it with a label
                    // read as the page resetting itself on every change.
                    Text {
                        anchors.centerIn: parent
                        visible: controller.loading && !controller.ready
                        text: "Loading preferences…"
                        color: Theme.textMuted
                        font.family: Theme.sans
                        font.pixelSize: 12
                        renderType: Text.NativeRendering
                    }
                }
            }

            Rectangle {
                anchors.fill: parent
                visible: controller.wallpaperPickerOpen
                color: Theme.scrim
                z: 50

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.ArrowCursor
                }
            }
        }

        Shortcut {
            sequence: "Escape"
            enabled: !controller.displayPending
            onActivated: preferences.dismissed()
        }
    }

    // Transient child of the preferences window, for the same reason as the
    // wallpaper picker: pinned to a screen name captured at open time, it stayed
    // on the original monitor once preferences could be dragged away.
    FloatingWindow {
        id: confirmation
        visible: controller.displayPending
        parentWindow: window
        title: "Keep Display Settings?"
        implicitWidth: 480
        implicitHeight: 190
        minimumSize: Qt.size(480, 190)
        maximumSize: Qt.size(480, 190)
        color: "transparent"
        surfaceFormat.opaque: false

        ContinuousRectangle {
            anchors.fill: parent
            color: Theme.contentTint

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 26
                spacing: 12

                Text {
                    Layout.fillWidth: true
                    text: "Keep these display settings?"
                    color: Theme.text
                    font.family: Theme.sans
                    font.pixelSize: 17
                    font.weight: Font.Bold
                    renderType: Text.NativeRendering
                }
                Text {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    text: "The previous display arrangement will be restored in "
                        + controller.displaySeconds + " seconds."
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 12
                    wrapMode: Text.WordWrap
                    renderType: Text.NativeRendering
                }
                RowLayout {
                    Layout.fillWidth: true
                    Item { Layout.fillWidth: true }
                    SettingsButton { text: "Revert"; ghost: true; onClicked: controller.revertDisplays() }
                    SettingsButton { text: "Keep Settings"; prominent: true; onClicked: controller.confirmDisplays() }
                }
            }
        }

        // Window-context shortcuts: the layer surface used to hold an exclusive
        // keyboard grab, a toplevel cannot. Hyprland focuses newly mapped windows,
        // and the 15s watchdog reverts regardless if focus ends up elsewhere.
        Shortcut { sequence: "Escape"; onActivated: controller.revertDisplays() }
        Shortcut { sequence: "Return"; onActivated: controller.confirmDisplays() }
    }
}
