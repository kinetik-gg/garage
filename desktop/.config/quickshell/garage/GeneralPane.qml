pragma ComponentBehavior: Bound

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
    readonly property var indexFrequencies: [1, 5, 15, 30, 60, 180, 720, 1440]
    readonly property var indexFrequencyLabels: [
        "Every minute", "Every 5 minutes", "Every 15 minutes", "Every 30 minutes",
        "Every hour", "Every 3 hours", "Every 12 hours", "Daily"
    ]
    readonly property var indexingDirectories: String(pane.controller.preference(
        "indexing", "directories", "")).split("\n").map(path => path.trim()).filter(Boolean)

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

    function displayDirectory(path) {
        const home = Quickshell.env("HOME");
        return String(path).replace(home, "~");
    }

    function removeIndexDirectory(path) {
        pane.controller.setPreference("indexing", "directories",
            pane.indexingDirectories.filter(item => item !== path).join("\n"));
    }

    function indexActivityLabel() {
        switch (pane.controller.indexStatus.activity) {
        case "indexing": return "Indexing…";
        case "idle": return "Up to date";
        case "disabled": return "Disabled";
        case "error": return "Needs attention";
        case "not_indexed": return "Not indexed yet";
        default: return "Checking…";
        }
    }

    function indexActivityDescription() {
        const status = pane.controller.indexStatus;
        if (status.error)
            return status.error;
        if (status.activity === "indexing")
            return "Building a new snapshot. Existing launcher results remain available.";
        if (status.activity === "disabled")
            return "Enable background indexing to search files from the launcher.";
        return "The background service scans filename and folder metadata only.";
    }

    function lastIndexDescription() {
        const status = pane.controller.indexStatus;
        if (!status.last_scan_epoch)
            return "No completed index is available yet.";
        const when = new Date(Number(status.last_scan_epoch) * 1000);
        const duration = Math.max(0, Number(status.last_scan_duration_ms || 0));
        return when.toLocaleString() + " · completed in " + duration + " ms";
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

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

        SettingsGroup {
            title: "FILE INDEXING"

            SettingsRow {
                title: "Background Indexing"
                description: "Keep filename and folder results ready for the launcher. File contents are never read."

                SettingsSwitch {
                    anchors.verticalCenter: parent.verticalCenter
                    checked: pane.controller.preference("indexing", "enabled", true)
                    onToggled: value => pane.controller.setPreference(
                        "indexing", "enabled", value)
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Index Activity"
                description: pane.indexActivityDescription()

                RowLayout {
                    spacing: 10

                    Text {
                        text: pane.indexActivityLabel()
                        color: Theme.text
                        font.family: Theme.sans
                        font.pixelSize: 12
                        font.weight: Font.Medium
                        renderType: Text.NativeRendering
                    }

                    SettingsButton {
                        text: pane.controller.indexRefreshing ? "Indexing…" : "Refresh Now"
                        iconSource: "icons/arrows-clockwise.svg"
                        enabled: pane.controller.preference("indexing", "enabled", true)
                            && !pane.controller.indexRefreshing
                        onClicked: pane.controller.refreshIndex()
                    }
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Last Index"
                description: pane.lastIndexDescription()

                Text {
                    text: Number(pane.controller.indexStatus.count || 0).toLocaleString()
                        + " items"
                    color: Theme.text
                    font.family: Theme.mono
                    font.pixelSize: 11
                    renderType: Text.NativeRendering
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Refresh Frequency"
                description: "How often the background index catches filesystem changes."
                rowEnabled: pane.controller.preference("indexing", "enabled", true)

                SettingsCombo {
                    implicitWidth: 190
                    enabled: pane.controller.preference("indexing", "enabled", true)
                    model: pane.indexFrequencyLabels
                    currentIndex: Math.max(0, pane.indexFrequencies.indexOf(
                        pane.controller.preference("indexing", "frequency_minutes", 5)))
                    onActivated: index => pane.controller.setPreference(
                        "indexing", "frequency_minutes", pane.indexFrequencies[index])
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Maximum Depth"
                description: "How many directory levels below each location are indexed."
                rowEnabled: pane.controller.preference("indexing", "enabled", true)

                RowLayout {
                    spacing: 10
                    SettingsSlider {
                        enabled: pane.controller.preference("indexing", "enabled", true)
                        from: 1
                        to: 64
                        stepSize: 1
                        value: pane.controller.preference("indexing", "max_depth", 8)
                        onCommitted: value => pane.controller.setPreference(
                            "indexing", "max_depth", Math.round(value))
                    }
                    Text {
                        text: String(Math.round(pane.controller.preference(
                            "indexing", "max_depth", 8)))
                        color: Theme.text
                        font.family: Theme.mono
                        font.pixelSize: 11
                        renderType: Text.NativeRendering
                    }
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Indexed Locations"
                description: "Only directories inside your home folder can be added."
                rowEnabled: pane.controller.preference("indexing", "enabled", true)

                SettingsButton {
                    text: "Add Folder…"
                    iconSource: "icons/folder.svg"
                    enabled: pane.controller.preference("indexing", "enabled", true)
                    onClicked: pane.controller.indexDirectoryPickerOpen = true
                }
            }

            Repeater {
                model: pane.indexingDirectories

                ColumnLayout {
                    id: indexDirectoryRow
                    required property int index
                    required property string modelData
                    Layout.fillWidth: true
                    spacing: 13

                    MenuSeparator { Layout.fillWidth: true }

                    SettingsRow {
                        title: pane.displayDirectory(indexDirectoryRow.modelData)
                        description: "Filename and folder metadata"
                        rowEnabled: pane.controller.preference("indexing", "enabled", true)

                        SettingsButton {
                            text: "Remove"
                            ghost: true
                            enabled: pane.controller.preference("indexing", "enabled", true)
                            onClicked: pane.removeIndexDirectory(indexDirectoryRow.modelData)
                        }
                    }
                }
            }
        }

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

        Item { Layout.preferredHeight: 20 }
    }
}
