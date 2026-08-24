import QtQuick
import QtQuick.Layouts
import Qt5Compat.GraphicalEffects

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property string background: pane.controller.preference(
        "bar", "background", "transparent")
    readonly property string position: pane.controller.preference("bar", "position", "top")

    // Anchor options in the order the segmented control shows them: right is
    // the default home for a widget, and left sits last because it is meant
    // for structural pieces like the menu, not the next thing installed.
    readonly property var anchorGroups: ["right", "center", "", "left"]

    // One row per widget the bar can draw: every stored id in rail order, then
    // the discovered extensions that are not placed anywhere. Ids that no
    // longer resolve to an extension stay listed rather than being dropped --
    // a stored id nobody can see or remove would otherwise be stuck in the
    // preference forever. An id stored in two rails lists once; the bar draws
    // rails left to right, so the first rail is the one shown here.
    readonly property var widgetRows: {
        const discovered = ExtensionRegistry.barWidgets || [];
        const byId = {};
        for (let index = 0; index < discovered.length; ++index)
            byId[discovered[index].id] = discovered[index];
        const rows = [];
        const seen = {};
        const groups = ["left", "center", "right"];
        for (let at = 0; at < groups.length; ++at) {
            const list = pane.controller.barList(groups[at]);
            for (let index = 0; index < list.length; ++index) {
                const id = list[index];
                if (seen[id])
                    continue;
                seen[id] = true;
                rows.push({
                    id: id,
                    name: byId[id] !== undefined ? byId[id].name : id,
                    group: groups[at],
                    index: index,
                    count: list.length,
                    known: byId[id] !== undefined
                });
            }
        }
        for (let index = 0; index < discovered.length; ++index) {
            const extension = discovered[index];
            if (!seen[extension.id])
                rows.push({ id: extension.id, name: extension.name,
                    group: "", index: -1, count: 0, known: true });
        }
        return rows;
    }

    // The nudge arrows beside each anchored widget. One glyph, flipped for
    // down: the icon set ships arrow-up only, and a mirrored copy would be
    // one more file to keep in step with it.
    component ReorderButton: ContinuousRectangle {
        id: nudge
        property bool down: false
        property bool enabled: true
        signal clicked()

        width: 28
        height: 32
        radius: Theme.controlRadius
        color: nudgePointer.containsMouse && nudge.enabled ? Theme.hoverStrong : Theme.hover
        borderWidth: 1
        borderColor: Theme.border
        opacity: nudge.enabled ? 1 : 0.35

        Image {
            id: nudgeGlyph
            anchors.centerIn: parent
            width: 13
            height: 13
            source: "icons/arrow-up.svg"
            sourceSize.width: 26
            sourceSize.height: 26
            smooth: true
            antialiasing: true
            mipmap: true
            visible: false
        }

        // The svg ships its own colour, so it needs an overlay to be
        // recoloured at all.
        ColorOverlay {
            anchors.fill: nudgeGlyph
            source: nudgeGlyph
            rotation: nudge.down ? 180 : 0
            color: Theme.text
            cached: true
        }

        MouseArea {
            id: nudgePointer
            anchors.fill: parent
            hoverEnabled: true
            enabled: nudge.enabled
            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: nudge.clicked()
        }
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "POSITION"

            SettingsRow {
                title: "Screen Edge"
                description: "Dragging the bar's background to another edge moves it too."
                SettingsSegmented {
                    model: ["Top", "Bottom", "Left", "Right"]
                    currentIndex: Math.max(0,
                        ["top", "bottom", "left", "right"].indexOf(pane.position))
                    onActivated: index => pane.controller.setPreference(
                        "bar", "position", ["top", "bottom", "left", "right"][index])
                }
            }
        }

        SettingsGroup {
            title: "APPEARANCE"

            SettingsRow {
                title: "Background"
                description: pane.background === "transparent"
                    ? "No blur behind the bar." : "Blurs the desktop behind the bar."
                SettingsSegmented {
                    model: ["Blurred", "Transparent"]
                    currentIndex: pane.background === "transparent" ? 1 : 0
                    onActivated: index => pane.controller.setPreference(
                        "bar", "background", index === 1 ? "transparent" : "blurred")
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Height"
                description: Math.round(pane.controller.preference("bar", "height", 43)) + " px"
                SettingsSlider {
                    from: 30
                    to: 60
                    stepSize: 1
                    value: pane.controller.preference("bar", "height", 43)
                    onCommitted: next => pane.controller.setPreference(
                        "bar", "height", Math.round(next))
                }
            }

            SettingsRow {
                title: "Padding"
                description: Number(pane.controller.preference(
                    "bar", "padding_scale", 1.2)).toFixed(2) + "×"
                SettingsSlider {
                    from: 1.0
                    to: 2.0
                    stepSize: 0.05
                    value: pane.controller.preference("bar", "padding_scale", 1.2)
                    onCommitted: next => pane.controller.setPreference(
                        "bar", "padding_scale", next)
                }
            }
        }

        // The rows come from what is actually installed, not a shipped list:
        // an extension dropped into an extensions root shows up here on its
        // own, with nothing in the backend knowing its name.
        SettingsGroup {
            title: "WIDGETS"

            SettingsRow {
                title: "Widgets per Section"
                description: "A section holding more than "
                    + Math.round(pane.controller.preference("bar", "max_group_widgets", 6))
                    + " widgets folds the rest behind a chevron."
                SettingsSlider {
                    from: 2
                    to: 16
                    stepSize: 1
                    value: pane.controller.preference("bar", "max_group_widgets", 6)
                    onCommitted: next => pane.controller.setPreference(
                        "bar", "max_group_widgets", Math.round(next))
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            Repeater {
                model: pane.widgetRows

                SettingsRow {
                    id: entry
                    required property var modelData
                    title: entry.modelData.name
                    rowEnabled: entry.modelData.known
                    description: !entry.modelData.known
                        ? "Nothing installed provides this widget."
                        : entry.modelData.group === "left"
                            ? "Left is for structural widgets; most belong on the right."
                            : ""

                    Row {
                        spacing: 6

                        ReorderButton {
                            visible: entry.modelData.known && entry.modelData.group !== ""
                            enabled: entry.modelData.index > 0
                            onClicked: pane.controller.barListMove(
                                entry.modelData.id, entry.modelData.group, -1)
                        }

                        ReorderButton {
                            down: true
                            visible: entry.modelData.known && entry.modelData.group !== ""
                            enabled: entry.modelData.index < entry.modelData.count - 1
                            onClicked: pane.controller.barListMove(
                                entry.modelData.id, entry.modelData.group, 1)
                        }

                        SettingsSegmented {
                            visible: entry.modelData.known
                            implicitWidth: 236
                            model: ["Right", "Center", "Off", "Left"]
                            currentIndex: pane.anchorGroups.indexOf(entry.modelData.group)
                            onActivated: index => {
                                const target = pane.anchorGroups[index];
                                if (target !== entry.modelData.group)
                                    pane.controller.barListSetGroup(
                                        entry.modelData.id, entry.modelData.group, target);
                            }
                        }

                        SettingsButton {
                            visible: !entry.modelData.known
                            text: "Remove"
                            onClicked: pane.controller.barListSetGroup(
                                entry.modelData.id, entry.modelData.group, "")
                        }
                    }
                }
            }
        }

        Item { Layout.preferredHeight: 20 }
    }
}
