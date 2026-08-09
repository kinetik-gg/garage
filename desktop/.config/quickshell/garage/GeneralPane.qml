import Quickshell
import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property var engineKeys: ["google", "duckduckgo", "bing", "kagi", "custom"]
    readonly property var engineLabels: ["Google", "DuckDuckGo", "Bing", "Kagi", "Custom…"]

    // The terminal is the one role with no registry behind it, so it is stored
    // as a preference rather than written to the mime defaults like the rest.
    readonly property var appRoles: [
        { role: "browser", title: "Web Browser", description: "Opens links and web searches." },
        { role: "mail", title: "Mail", description: "Opens mailto: links." },
        { role: "files", title: "File Manager", description: "Opens folders." },
        { role: "editor", title: "Text Editor", description: "Opens plain text files." },
        { role: "terminal", title: "Terminal", description: "Runs the shell keybindings." },
        { role: "image", title: "Image Viewer", description: "Opens PNG and JPEG images." },
        { role: "video", title: "Video Player", description: "Opens MP4 and Matroska video." },
        { role: "pdf", title: "PDF Viewer", description: "Opens PDF documents." }
    ]

    function roleInfo(role) {
        const apps = pane.controller.snapshot.defaultApps;
        return (apps && apps[role]) || { current: "", candidates: [] };
    }

    // Quickshell scans the desktop entries lazily, and byId() registers no
    // dependency on that scan. Read the count instead: it makes the labels
    // resolve again once the entries arrive, and a pane opened before anything
    // else has touched DesktopEntries would otherwise be a column of bare ids.
    readonly property int applicationCount: DesktopEntries.applications.values.length

    // The display name the application ships, which is what the launcher
    // already shows. The id without its suffix is the fallback for an entry
    // Quickshell cannot resolve, so a row never goes blank.
    function appLabel(desktopId) {
        const bare = String(desktopId).replace(/\.desktop$/, "");
        const entry = pane.applicationCount > 0 ? DesktopEntries.byId(bare) : null;
        return entry && entry.name ? entry.name : bare;
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "DEFAULT APPLICATIONS"

            Repeater {
                model: pane.appRoles

                ColumnLayout {
                    id: appRow
                    required property int index
                    required property var modelData
                    readonly property var info: pane.roleInfo(modelData.role)

                    Layout.fillWidth: true
                    spacing: 13

                    MenuSeparator {
                        Layout.fillWidth: true
                        visible: appRow.index > 0
                    }

                    SettingsRow {
                        title: appRow.modelData.title
                        description: appRow.modelData.description
                        // Nothing registered for the role at all. Saying so
                        // reads better than a combo with nothing in it.
                        rowEnabled: appRow.info.candidates.length > 0

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: appRow.info.candidates.length === 0
                            text: "None installed"
                            color: Theme.textMuted
                            font.family: Theme.sans
                            font.pixelSize: 12
                            renderType: Text.NativeRendering
                        }

                        SettingsCombo {
                            visible: appRow.info.candidates.length > 0
                            // One candidate is not a choice: PDF has only the
                            // browser here. Show which application it is, but
                            // do not offer a menu of one.
                            enabled: appRow.info.candidates.length > 1
                            implicitWidth: 220
                            model: appRow.info.candidates.map(pane.appLabel)
                            currentIndex: appRow.info.candidates.indexOf(appRow.info.current)
                            onActivated: index => {
                                const chosen = appRow.info.candidates[index];
                                if (appRow.modelData.role === "terminal")
                                    pane.controller.setPreference("general", "terminal", chosen);
                                else
                                    pane.controller.action("defaults." + appRow.modelData.role, chosen);
                            }
                        }
                    }
                }
            }
        }

        SettingsGroup {
            title: "LAUNCHER"

            SettingsRow {
                title: "Use Built-in Launcher"
                description: "When off, Super+Space runs rofi or wofi instead."

                SettingsSwitch {
                    anchors.verticalCenter: parent.verticalCenter
                    checked: pane.controller.preference("general", "builtin_launcher", true)
                    onToggled: value => pane.controller.setPreference(
                        "general", "builtin_launcher", value)
                }
            }
        }

        SettingsGroup {
            title: "SEARCH"

            SettingsRow {
                title: "Search Engine"
                description: "Used when the launcher searches the web."
                SettingsCombo {
                    model: pane.engineLabels
                    currentIndex: Math.max(0, pane.engineKeys.indexOf(
                        pane.controller.preference("appearance", "search_engine", "google")))
                    onActivated: index => pane.controller.setPreference(
                        "appearance", "search_engine", pane.engineKeys[index])
                }
            }

            MenuSeparator {
                Layout.fillWidth: true
                visible: pane.controller.preference("appearance", "search_engine", "google") === "custom"
            }

            SettingsRow {
                title: "Custom URL"
                description: "Use %s where the query belongs."
                visible: pane.controller.preference("appearance", "search_engine", "google") === "custom"

                ContinuousRectangle {
                    width: 260
                    height: 32
                    radius: Theme.controlRadius
                    color: Theme.surface
                    borderWidth: 1
                    borderColor: Theme.border

                    TextInput {
                        anchors.fill: parent
                        anchors.margins: 8
                        text: pane.controller.preference("appearance", "search_custom_url", "")
                        color: Theme.text
                        selectionColor: Theme.accent
                        selectedTextColor: Theme.accentText
                        font.family: Theme.mono
                        font.pixelSize: 11
                        verticalAlignment: Text.AlignVCenter
                        selectByMouse: true
                        clip: true
                        onEditingFinished: pane.controller.setPreference(
                            "appearance", "search_custom_url", text)
                    }
                }
            }
        }

        Item { Layout.preferredHeight: 20 }
    }
}
