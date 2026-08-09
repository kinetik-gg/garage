import QtQuick
import QtQuick.Layouts

Item {
    id: pane
    required property var controller

    readonly property var shortcuts: controller.snapshot.keybindings
        || ({ available: false, groups: [], custom: [], modifiers: [], keys: [] })
    readonly property bool changed: (shortcuts.groups || []).some(
        group => group.binds.some(bind => bind.modified))

    // How a combination is written for a human. The stored form is what
    // Hyprland's parser wants -- xkb keysym names, mouse:272, XF86 media keys --
    // and "bracketleft" is not a thing to show anybody.
    readonly property var tokenLabels: ({
        SUPER: "Super", CONTROL: "Control", CTRL: "Control", ALT: "Alt", SHIFT: "Shift",
        CAPS: "Caps Lock", MOD5: "AltGr",
        minus: "-", Minus: "-", plus: "+", Plus: "+", equal: "=",
        bracketleft: "[", bracketright: "]", backslash: "\\", semicolon: ";",
        apostrophe: "'", grave: "`", comma: ",", period: ".", slash: "/",
        Page_Up: "Page Up", Page_Down: "Page Down", BackSpace: "Backspace",
        Scroll_Lock: "Scroll Lock", mouse_up: "Scroll Up", mouse_down: "Scroll Down",
        "mouse:272": "Left Click", "mouse:273": "Right Click"
    })

    // A shell command legitimately carries newlines and tabs, and a row is one
    // line high: left alone they push the rest of the row off the edge instead
    // of eliding. Flattened for display only -- the stored command keeps them.
    function oneLine(text) {
        return String(text).replace(/\s+/g, " ").trim();
    }

    function keyLabel(combination) {
        return String(combination).split("+").map(part => {
            const token = part.trim();
            const named = pane.tokenLabels[token] || pane.tokenLabels[token.toUpperCase()];
            if (named)
                return named;
            // Media keys arrive as one run-together word; a keycode is a key
            // whose name Hyprland never resolved, so there is nothing to show
            // but the number.
            if (token.startsWith("XF86"))
                return token.slice(4).replace(/([a-z0-9])([A-Z])/g, "$1 $2");
            if (token.startsWith("code:"))
                return "Key " + token.slice(5);
            return token;
        }).join(" + ");
    }

    // The editor, held here rather than in a component of its own: it is one
    // dialog serving three jobs -- move a shortcut, invent one, change one that
    // was invented -- and they differ only in which fields are shown.
    property bool editorOpen: false
    property string editorMode: "rebind"
    property string editorId: ""
    property string editorAction: ""
    property string editorName: ""
    property string editorCommand: ""
    property string editorKey: "A"
    property bool modSuper: false
    property bool modControl: false
    property bool modAlt: false
    property bool modShift: false

    readonly property string editorCombination: {
        const parts = [];
        if (modSuper) parts.push("SUPER");
        if (modControl) parts.push("CONTROL");
        if (modAlt) parts.push("ALT");
        if (modShift) parts.push("SHIFT");
        parts.push(editorKey);
        return parts.join(" + ");
    }
    // The whitelist comes from the backend, so the list the pane offers and the
    // set parse_combination() accepts cannot drift. The key a shortcut is
    // already on is carried in when it is not on the list -- a keypad code or a
    // media key -- so opening the editor cannot silently move it.
    readonly property var keyChoices: {
        const choices = (pane.shortcuts.keys || []).slice();
        if (pane.editorKey !== "" && choices.indexOf(pane.editorKey) < 0)
            choices.unshift(pane.editorKey);
        return choices;
    }

    function loadCombination(keys) {
        const parts = String(keys).split("+").map(part => part.trim()).filter(part => part !== "");
        pane.modSuper = pane.modControl = pane.modAlt = pane.modShift = false;
        pane.editorKey = parts.length ? parts[parts.length - 1] : "A";
        for (let index = 0; index < parts.length - 1; ++index) {
            const name = parts[index].toUpperCase();
            if (name === "SUPER" || name === "MOD4")
                pane.modSuper = true;
            else if (name === "CONTROL" || name === "CTRL")
                pane.modControl = true;
            else if (name === "ALT")
                pane.modAlt = true;
            else if (name === "SHIFT")
                pane.modShift = true;
        }
    }

    function openRebind(bind) {
        editorMode = "rebind";
        editorId = bind.id;
        editorAction = bind.description;
        loadCombination(bind.keys);
        editorOpen = true;
    }

    function openCustom(item) {
        editorMode = item ? "edit" : "add";
        editorId = item ? item.id : "";
        editorAction = "";
        editorName = item ? item.description : "";
        editorCommand = item ? item.command : "";
        loadCombination(item ? item.keys : "SUPER + SHIFT + F1");
        editorOpen = true;
    }

    function commit() {
        if (editorMode === "rebind")
            controller.action("keybind.rebind", { id: editorId, keys: editorCombination });
        else if (editorMode === "add")
            controller.action("keybind.add", { keys: editorCombination,
                description: editorName, command: editorCommand });
        else
            controller.action("keybind.update", { id: editorId, keys: editorCombination,
                description: editorName, command: editorCommand });
        editorOpen = false;
    }

    Flickable {
        anchors.fill: parent
        contentHeight: content.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        ColumnLayout {
            id: content
            width: parent.width
            spacing: 22

            // Only before the compositor has ever published the list. The
            // shortcuts still work; there is simply nothing yet to draw them
            // from, and the next reload writes it.
            SettingsGroup {
                visible: !pane.shortcuts.available
                title: "KEYBOARD"

                SettingsRow {
                    title: "Shortcuts have not been listed yet"
                    description: "config/binds.lua publishes the list each time Hyprland reads its "
                        + "configuration. Reload Hyprland and open this pane again."
                }
            }

            Repeater {
                model: pane.shortcuts.groups || []

                SettingsGroup {
                    required property var modelData
                    title: modelData.title.toUpperCase()

                    Repeater {
                        model: modelData.binds

                        KeybindRow {
                            required property var modelData
                            title: modelData.description
                            subtitle: modelData.protected
                                ? "Kept free so a terminal is always reachable"
                                : modelData.modified
                                ? "Default: " + pane.keyLabel(modelData.defaultKeys) : ""
                            keys: pane.keyLabel(modelData.keys)
                            editable: !modelData.protected
                            modified: modelData.modified
                            resettable: modelData.modified
                            onEdit: pane.openRebind(modelData)
                            onReset: pane.controller.action("keybind.reset", { id: modelData.id })
                        }
                    }
                }
            }

            SettingsGroup {
                visible: pane.shortcuts.available
                title: "CUSTOM SHORTCUTS"

                Repeater {
                    model: pane.shortcuts.custom || []

                    KeybindRow {
                        required property var modelData
                        title: pane.oneLine(modelData.description !== ""
                            ? modelData.description : modelData.command)
                        subtitle: pane.oneLine(modelData.command)
                        keys: pane.keyLabel(modelData.keys)
                        resetLabel: "Remove"
                        resettable: true
                        onEdit: pane.openCustom(modelData)
                        onReset: pane.controller.action("keybind.remove", { id: modelData.id })
                    }
                }

                SettingsRow {
                    title: (pane.shortcuts.custom || []).length === 0 ? "No custom shortcuts" : ""
                    description: "A custom shortcut runs a command line through /bin/sh, the same "
                        + "way a terminal would."
                    SettingsButton {
                        text: "Add Shortcut"
                        onClicked: pane.openCustom(null)
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                visible: pane.changed

                Text {
                    Layout.fillWidth: true
                    Layout.leftMargin: 4
                    text: "Shortcuts you have moved are shown with the combination they came with."
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    renderType: Text.NativeRendering
                }

                SettingsButton {
                    text: "Restore All"
                    onClicked: pane.controller.action("keybind.reset-all")
                }
            }

            Item { Layout.preferredHeight: 20 }
        }
    }

    // Over the pane rather than in a window of its own: a second toplevel would
    // be a third place for the compositor to put a window, and the wallpaper
    // picker's scrim already establishes this as how a modal reads here.
    Rectangle {
        anchors.fill: parent
        visible: pane.editorOpen
        color: Theme.scrim

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.ArrowCursor
            onClicked: pane.editorOpen = false
        }
    }

    ContinuousRectangle {
        anchors.centerIn: parent
        width: Math.min(parent.width - 24, 420)
        implicitHeight: editor.implicitHeight + 40
        visible: pane.editorOpen
        // Opaque, not one of the translucent surfaces. Those read as glass
        // because the compositor blurs a real window behind them; this sits
        // inside one, so the same colour only shows the rows underneath.
        color: Theme.bodyRaised
        borderWidth: 1
        borderColor: Theme.border

        // Swallows the clicks the scrim below would otherwise take as dismiss.
        MouseArea { anchors.fill: parent }

        ColumnLayout {
            id: editor
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.margins: 20
            spacing: 14

            Text {
                Layout.fillWidth: true
                text: pane.editorMode === "rebind" ? "Change Shortcut"
                    : pane.editorMode === "add" ? "New Custom Shortcut" : "Edit Custom Shortcut"
                color: Theme.text
                font.family: Theme.sans
                font.pixelSize: 15
                font.weight: Font.Bold
                renderType: Text.NativeRendering
            }

            Text {
                Layout.fillWidth: true
                visible: pane.editorAction !== ""
                text: pane.editorAction
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                wrapMode: Text.WordWrap
                renderType: Text.NativeRendering
            }

            ColumnLayout {
                Layout.fillWidth: true
                visible: pane.editorMode !== "rebind"
                spacing: 8

                Repeater {
                    model: [
                        { label: "Name", placeholder: "Optional", custom: false },
                        { label: "Command", placeholder: "e.g. gnome-screenshot -i", custom: true }
                    ]

                    RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 12

                        Text {
                            Layout.preferredWidth: 70
                            text: modelData.label
                            color: Theme.textMuted
                            font.family: Theme.sans
                            font.pixelSize: 11
                            renderType: Text.NativeRendering
                        }

                        ContinuousRectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 30
                            radius: Theme.controlRadius
                            color: Theme.surface
                            borderWidth: 1
                            borderColor: Theme.border

                            TextInput {
                                anchors.fill: parent
                                anchors.margins: 8
                                // Seeded from the pane rather than bound to it:
                                // a two-way binding on the same property is
                                // broken by the first keystroke anyway, and the
                                // editor is reopened for every edit.
                                Component.onCompleted: text = modelData.custom
                                    ? pane.editorCommand : pane.editorName
                                color: Theme.text
                                selectionColor: Theme.accent
                                selectedTextColor: Theme.accentText
                                font.family: modelData.custom ? Theme.mono : Theme.sans
                                font.pixelSize: 11
                                verticalAlignment: Text.AlignVCenter
                                selectByMouse: true
                                clip: true
                                onTextChanged: {
                                    if (modelData.custom)
                                        pane.editorCommand = text;
                                    else
                                        pane.editorName = text;
                                }

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: parent.text === ""
                                    text: modelData.placeholder
                                    color: Theme.textDisabled
                                    font: parent.font
                                    renderType: Text.NativeRendering
                                }
                            }
                        }
                    }
                }
            }

            // Modifier buttons and a key list rather than "press the shortcut
            // now". A capture field cannot see a combination the compositor
            // already binds -- pressing SUPER+W over this window would close it
            // -- and the only way to make it see one is to switch to an empty
            // submap, which is itself the lockout this pane exists to avoid.
            RowLayout {
                Layout.fillWidth: true
                spacing: 12

                Text {
                    Layout.preferredWidth: 70
                    text: "Shortcut"
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 11
                    renderType: Text.NativeRendering
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: 6

                    SettingsButton {
                        text: "Super"; horizontalPadding: 10; verticalPadding: 5
                        prominent: pane.modSuper
                        onClicked: pane.modSuper = !pane.modSuper
                    }
                    SettingsButton {
                        text: "Control"; horizontalPadding: 10; verticalPadding: 5
                        prominent: pane.modControl
                        onClicked: pane.modControl = !pane.modControl
                    }
                    SettingsButton {
                        text: "Alt"; horizontalPadding: 10; verticalPadding: 5
                        prominent: pane.modAlt
                        onClicked: pane.modAlt = !pane.modAlt
                    }
                    SettingsButton {
                        text: "Shift"; horizontalPadding: 10; verticalPadding: 5
                        prominent: pane.modShift
                        onClicked: pane.modShift = !pane.modShift
                    }
                    SettingsCombo {
                        implicitWidth: 140
                        model: pane.keyChoices
                        currentIndex: pane.keyChoices.indexOf(pane.editorKey)
                        onActivated: index => pane.editorKey = pane.keyChoices[index]
                    }
                }
            }

            Text {
                Layout.fillWidth: true
                text: pane.keyLabel(pane.editorCombination)
                color: Theme.text
                font.family: Theme.sans
                font.pixelSize: 13
                font.weight: Font.Medium
                horizontalAlignment: Text.AlignHCenter
                renderType: Text.NativeRendering
            }

            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                SettingsButton { text: "Cancel"; ghost: true; onClicked: pane.editorOpen = false }
                SettingsButton {
                    text: "Save"
                    prominent: true
                    // The backend refuses a clash, a protected shortcut and a
                    // key that would be swallowed everywhere, and reports why in
                    // the banner above. Only the one condition it cannot phrase
                    // usefully -- nothing to run -- is caught here.
                    enabled: pane.editorMode === "rebind" || pane.editorCommand.trim() !== ""
                    onClicked: pane.commit()
                }
            }
        }
    }
}
