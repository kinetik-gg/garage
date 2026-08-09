import Quickshell.Io
import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    property var modules: []
    property bool loading: true
    property string error: ""
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property var ignoredModules: ["Title", "Separator", "Break", "Colors"]

    function readableKey(key) {
        return String(key)
            .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
            .replace(/[_-]/g, " ")
            .replace(/^./, character => character.toUpperCase());
    }

    function formatBytes(bytes) {
        const units = ["B", "KB", "MB", "GB", "TB", "PB"];
        let value = Number(bytes);
        let unit = 0;
        while (value >= 1024 && unit < units.length - 1) { value /= 1024; ++unit; }
        return value.toFixed(unit > 2 ? 1 : 0).replace(".0", "") + " " + units[unit];
    }

    function primitiveValue(value, moduleName) {
        if (typeof value === "boolean") return value ? "Yes" : "No";
        if (typeof value === "number" && value > 1048576
                && ["Memory", "Swap", "Disk", "GPU"].includes(moduleName))
            return formatBytes(value);
        return String(value);
    }

    function flatten(value, prefix, moduleName, rows) {
        if (value === null || value === undefined || value === "") return;
        if (Array.isArray(value)) {
            for (let index = 0; index < value.length; ++index)
                flatten(value[index], prefix + " " + (index + 1), moduleName, rows);
            return;
        }
        if (typeof value === "object") {
            for (const key of Object.keys(value)) {
                const label = prefix ? prefix + " · " + readableKey(key) : readableKey(key);
                flatten(value[key], label, moduleName, rows);
            }
            return;
        }
        rows.push({ label: prefix || moduleName, value: primitiveValue(value, moduleName) });
    }

    function loadFastfetch(text) {
        try {
            const data = JSON.parse(text);
            const result = [];
            for (const item of data) {
                if (ignoredModules.includes(item.type) || item.result === null
                        || item.result === undefined || item.error) continue;
                const rows = [];
                flatten(item.result, "", item.type, rows);
                if (rows.length) result.push({ title: item.type, rows: rows });
            }
            modules = result;
        } catch (failure) {
            error = String(failure);
        }
        loading = false;
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 14

        RowLayout {
            Layout.fillWidth: true
            Image {
                Layout.preferredWidth: 58
                Layout.preferredHeight: 58
                source: "icons/archlinux-logo.svg"
                sourceSize.width: 116
                sourceSize.height: 116
                fillMode: Image.PreserveAspectFit
                smooth: true
                antialiasing: true
                mipmap: true
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 3
                Text {
                    Layout.fillWidth: true
                    text: "System Information"
                    color: Theme.text
                    font.family: Theme.sans
                    font.pixelSize: 15
                    font.weight: Font.Medium
                    renderType: Text.NativeRendering
                }
                Text {
                    Layout.fillWidth: true
                    text: loading ? "Collecting hardware and software details with Fastfetch…"
                        : modules.length + " information categories reported by Fastfetch"
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    renderType: Text.NativeRendering
                }
            }
        }

        Text {
            Layout.fillWidth: true
            visible: error !== ""
            text: error
            color: Theme.textMuted
            font.family: Theme.mono
            font.pixelSize: 11
            wrapMode: Text.WordWrap
            renderType: Text.NativeRendering
        }

        Repeater {
            model: pane.modules
            SettingsGroup {
                id: moduleGroup
                required property var modelData
                title: modelData.title.toUpperCase()

                Repeater {
                    model: moduleGroup.modelData.rows
                    SettingsRow {
                        required property var modelData
                        title: modelData.label
                        // TextEdit rather than Text: the values are worth copying,
                        // and selection is a TextEdit capability. Assigning
                        // selectByMouse to a Text failed to compile the whole
                        // component, which is why this pane rendered blank.
                        TextEdit {
                            width: Math.min(240, implicitWidth)
                            text: modelData.value
                            readOnly: true
                            color: Theme.text
                            selectionColor: Theme.accent
                            selectedTextColor: Theme.accentText
                            font.family: Theme.sans
                            font.pixelSize: 12
                            horizontalAlignment: Text.AlignRight
                            wrapMode: Text.WrapAnywhere
                            selectByMouse: true
                            renderType: Text.NativeRendering
                        }
                    }
                }
            }
        }

        Text {
            Layout.fillWidth: true
            Layout.topMargin: 8
            text: "The Arch Linux™ name and logo are recognized trademarks. Linux® is the "
                + "registered trademark of Linus Torvalds. GNU software is developed by the "
                + "Free Software Foundation and contributors."
            color: Theme.textDisabled
            font.family: Theme.sans
            font.pixelSize: 9
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            renderType: Text.NativeRendering
        }
        Item { Layout.preferredHeight: 18 }
    }

    Process {
        command: ["fastfetch", "--format", "json"]
        running: true
        stdout: StdioCollector { onStreamFinished: pane.loadFastfetch(text) }
    }
}
