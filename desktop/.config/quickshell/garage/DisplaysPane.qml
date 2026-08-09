import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    property var layout: []
    property int selectedIndex: 0
    property bool dirty: false
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    function clone(value) { return JSON.parse(JSON.stringify(value)); }

    function reload() {
        if (dirty || controller.displayPending)
            return;
        layout = clone(controller.snapshot.displays || []);
        if (selectedIndex >= layout.length)
            selectedIndex = Math.max(0, layout.length - 1);
    }

    function updateDisplay(index, key, value) {
        const next = clone(layout);
        if (!next[index]) return;
        next[index][key] = value;
        if (key === "primary" && value) {
            for (let i = 0; i < next.length; ++i)
                next[i].primary = i === index;
        }
        if (key === "mode") {
            const match = String(value).match(/^(\d+)x(\d+)@/);
            if (match) {
                next[index].width = Number(match[1]);
                next[index].height = Number(match[2]);
            }
        }
        layout = pruneMirrors(next);
        dirty = true;
    }

    function mirrorOf(item) { return item ? String(item.mirror || "") : ""; }

    function displayFor(output) {
        return layout.find(item => String(item.output) === String(output)) || null;
    }

    // Physical pixels after rotation. Hyprland fits a mirror from the panel's
    // transformed size, which 90 and 270 degree rotations swap.
    function panelSize(item) {
        const width = Number(item.width || 1);
        const height = Number(item.height || 1);
        return Number(item.transform || 0) % 2
            ? { width: height, height: width } : { width: width, height: height };
    }

    // The outputs a display may copy: enabled, not itself, and showing the
    // desktop rather than another mirror. Hyprland's setMirror refuses the rest
    // outright, so offering them would only produce a rule it drops.
    function mirrorSources(index) {
        const item = layout[index];
        if (!item) return [];
        return layout.filter(other => String(other.output) !== String(item.output)
            && other.enabled !== false && !mirrorOf(other)).map(other => String(other.output));
    }

    // What this display actually mirrors once the same rules are applied, or ""
    // when it stands on its own. The primary is excluded because something has
    // to keep showing the desktop.
    function mirrorSource(index) {
        const item = layout[index];
        if (!item || item.enabled === false || item.primary) return "";
        const source = displayFor(mirrorOf(item));
        return source && source.enabled !== false && !mirrorOf(source)
            ? String(source.output) : "";
    }

    // Drops any mirror an edit has just invalidated -- turning the source off,
    // making the mirror primary, or pointing two displays at each other -- so
    // the pane never offers to apply a layout the backend would reject.
    function pruneMirrors(next) {
        const byOutput = {};
        for (let i = 0; i < next.length; ++i)
            byOutput[String(next[i].output)] = next[i];
        for (let i = 0; i < next.length; ++i) {
            const item = next[i];
            if (!item.mirror) continue;
            const source = byOutput[String(item.mirror)];
            if (item.primary || item.enabled === false || !source
                    || source === item || source.enabled === false || source.mirror)
                item.mirror = "";
        }
        return next;
    }

    // A mirror occupies its source's exact rectangle, so two cards would stack
    // and dragging either would be a guess. They get a chip under the canvas.
    function inArrangement(index) { return mirrorSource(index) === ""; }

    function mirrorCount() {
        let count = 0;
        for (let i = 0; i < layout.length; ++i)
            if (!inArrangement(i)) ++count;
        return count;
    }

    // Hyprland's renderMirrored() scales the source's framebuffer to fit this
    // panel and centres it, so the two never have to share a mode -- but a
    // different shape gets black bars and a bigger source arrives softened.
    // Say which, rather than let the result look like a fault.
    function mirrorNote(index) {
        const item = layout[index];
        if (!item) return "";
        if (item.enabled === false) return "Turn this display on to mirror another.";
        if (item.primary) return "The primary display always shows the desktop.";
        const source = displayFor(mirrorOf(item));
        if (!source || mirrorSource(index) === "")
            return mirrorSources(index).length > 0
                ? "Shows its own part of the desktop."
                : "No other display is available to mirror.";
        const own = panelSize(item);
        const from = panelSize(source);
        const label = String(source.model || source.output) + " (" + source.output + ")";
        if (Math.abs(own.width / own.height - from.width / from.height) > 0.01)
            return "Copies " + label + " with black bars: " + from.width + "×" + from.height
                + " does not fit the shape of this " + own.width + "×" + own.height + " panel.";
        const factor = own.width / from.width;
        return "Copies " + label + ", " + (factor < 0.999 ? "scaled down"
            : factor > 1.001 ? "scaled up" : "pixel for pixel")
            + " to " + own.width + "×" + own.height + ".";
    }

    // -1 hands the display back to misc:vrr in the Hyprland config, which is
    // what every display does until one is set here. Positional with vrrModes.
    readonly property var vrrModes: [-1, 0, 1, 2, 3]
    readonly property var vrrNotes: [
        "Follows the system setting.",
        "Always drives the panel at its fixed refresh rate.",
        "Matches the panel to the frame rate at all times.",
        "Matches the panel only while a window is fullscreen.",
        "Matches the panel only for fullscreen games and video."
    ]

    function moveDisplay(index, x, y) {
        const next = clone(layout);
        const moving = next[index];
        const ownWidth = Number(moving.width || 1) / Number(moving.scale || 1);
        const ownHeight = Number(moving.height || 1) / Number(moving.scale || 1);
        let best = null;
        function collides(candidateX, candidateY) {
            const right = candidateX + ownWidth;
            const bottom = candidateY + ownHeight;
            for (let i = 0; i < next.length; ++i) {
                if (i === index || next[i].enabled === false || !inArrangement(i)) continue;
                const other = next[i];
                const otherLeft = Number(other.x || 0);
                const otherTop = Number(other.y || 0);
                const otherRight = otherLeft
                    + Number(other.width || 1) / Number(other.scale || 1);
                const otherBottom = otherTop
                    + Number(other.height || 1) / Number(other.scale || 1);
                if (candidateX < otherRight - 0.5 && right > otherLeft + 0.5
                        && candidateY < otherBottom - 0.5 && bottom > otherTop + 0.5)
                    return true;
            }
            return false;
        }
        function consider(candidateX, candidateY) {
            if (collides(candidateX, candidateY)) return;
            const distance = Math.pow(x - candidateX, 2) + Math.pow(y - candidateY, 2);
            if (best === null || distance < best.distance)
                best = { x: Math.round(candidateX), y: Math.round(candidateY), distance: distance };
        }
        function snapOrKeep(value, candidates, minimum, maximum) {
            let result = Math.max(minimum, Math.min(maximum, value));
            let closest = 31;
            for (const candidate of candidates) {
                const distance = Math.abs(value - candidate);
                if (distance < closest) { closest = distance; result = candidate; }
            }
            return result;
        }
        for (let i = 0; i < next.length; ++i) {
            if (i === index || next[i].enabled === false || !inArrangement(i)) continue;
            const other = next[i];
            const left = Number(other.x || 0);
            const top = Number(other.y || 0);
            const right = left + Number(other.width || 1) / Number(other.scale || 1);
            const bottom = top + Number(other.height || 1) / Number(other.scale || 1);
            const otherWidth = right - left;
            const otherHeight = bottom - top;
            const alignedY = snapOrKeep(y,
                [top, top + (otherHeight - ownHeight) / 2, bottom - ownHeight],
                top - ownHeight + 1, bottom - 1);
            const alignedX = snapOrKeep(x,
                [left, left + (otherWidth - ownWidth) / 2, right - ownWidth],
                left - ownWidth + 1, right - 1);
            consider(right, alignedY);
            consider(left - ownWidth, alignedY);
            consider(alignedX, bottom);
            consider(alignedX, top - ownHeight);
        }
        moving.x = best ? best.x : Math.round(x);
        moving.y = best ? best.y : Math.round(y);
        layout = next;
        dirty = true;
    }

    readonly property int canvasPadding: 14

    function enabledCount() {
        return layout.filter(item => item.enabled !== false).length;
    }

    function minimumX() {
        let result = null;
        for (let i = 0; i < layout.length; ++i)
            if (inArrangement(i))
                result = result === null ? Number(layout[i].x || 0)
                    : Math.min(result, Number(layout[i].x || 0));
        return result === null ? 0 : result;
    }

    function minimumY() {
        let result = null;
        for (let i = 0; i < layout.length; ++i)
            if (inArrangement(i))
                result = result === null ? Number(layout[i].y || 0)
                    : Math.min(result, Number(layout[i].y || 0));
        return result === null ? 0 : result;
    }

    // Extent of the arranged displays in layout coordinates.
    function canvasSpan() {
        let maxX = 1;
        let maxY = 1;
        for (let i = 0; i < layout.length; ++i) {
            if (!inArrangement(i)) continue;
            maxX = Math.max(maxX, Number(layout[i].x || 0)
                + Number(layout[i].width || 1) / Number(layout[i].scale || 1));
            maxY = Math.max(maxY, Number(layout[i].y || 0)
                + Number(layout[i].height || 1) / Number(layout[i].scale || 1));
        }
        return { width: Math.max(1, maxX - minimumX()),
                 height: Math.max(1, maxY - minimumY()) };
    }

    // One factor for both axes and for position as well as size, so the cards
    // stay a scale drawing of the real arrangement: a 4K panel next to a 1080p
    // one reads as twice the size, and nothing can overlap that does not
    // overlap for real.
    function canvasScale() {
        if (!layout.length) return 0.08;
        const span = canvasSpan();
        const room = 2 * canvasPadding;
        return Math.min(0.12, (arrangement.width - room) / span.width,
            (arrangement.height - room) / span.height);
    }

    // Centre the whole arrangement rather than pinning it to a corner, so a
    // stacked column does not sit against one edge with dead space beside it.
    function canvasOffsetX() {
        const span = canvasSpan();
        return Math.max(0, (arrangement.width - 2 * canvasPadding
            - span.width * canvasScale()) / 2);
    }

    function canvasOffsetY() {
        const span = canvasSpan();
        return Math.max(0, (arrangement.height - 2 * canvasPadding
            - span.height * canvasScale()) / 2);
    }

    function testLayout() {
        const primary = layout.find(item => item.primary) || layout.find(item => item.enabled !== false);
        controller.testDisplays({ primary: primary ? primary.output : "", displays: clone(layout) });
    }

    function selected(key, fallback) {
        const item = layout[selectedIndex];
        return item && item[key] !== undefined ? item[key] : fallback;
    }

    Component.onCompleted: reload()
    Connections {
        target: controller
        function onChanged() { pane.reload(); }
        function onDisplayPendingChanged() {
            if (pane.controller.displayPending)
                pane.dirty = false;
            else
                pane.reload();
        }
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "ARRANGEMENT"

            Rectangle {
                id: arrangement
                Layout.fillWidth: true
                Layout.preferredHeight: 160
                color: "transparent"
                // Cards are scaled to fit, so nothing should reach the edge.
                // Kept as a backstop while a card is mid-drag.
                clip: true

                Text {
                    anchors.centerIn: parent
                    visible: pane.layout.length === 0
                    text: "No displays detected"
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 12
                    renderType: Text.NativeRendering
                }

                Repeater {
                    model: pane.layout
                    // A plain Rectangle with square corners: this is a scale
                    // drawing of a panel, not a piece of shell chrome.
                    Rectangle {
                        id: monitorCard
                        required property int index
                        required property var modelData
                        readonly property real factor: pane.canvasScale()
                        x: pane.canvasPadding + pane.canvasOffsetX()
                            + (Number(modelData.x || 0) - pane.minimumX()) * factor
                        y: pane.canvasPadding + pane.canvasOffsetY()
                            + (Number(modelData.y || 0) - pane.minimumY()) * factor
                        // No minimum size. Clamping one made small panels stop
                        // shrinking while their positions kept scaling, so cards
                        // drew over their neighbours and every display looked the
                        // same size regardless of resolution.
                        width: Number(modelData.width || 800) / Number(modelData.scale || 1) * factor
                        height: Number(modelData.height || 600) / Number(modelData.scale || 1) * factor
                        radius: 0
                        color: index === pane.selectedIndex ? Theme.accentSoft : Theme.surface
                        border.width: index === pane.selectedIndex ? 6 : 3
                        border.color: index === pane.selectedIndex ? Theme.accent : Theme.border
                        antialiasing: false
                        opacity: modelData.enabled === false ? 0.42 : 1
                        visible: pane.inArrangement(index)

                        Text {
                            anchors.centerIn: parent
                            width: parent.width - 10
                            // Below this the label is a smear of ellipsis, and the
                            // card reads better as a bare rectangle.
                            visible: parent.width >= 46 && parent.height >= 26
                            text: monitorCard.modelData.model || monitorCard.modelData.output
                            color: monitorCard.index === pane.selectedIndex
                                ? Theme.accentText : Theme.text
                            font.family: Theme.sans
                            font.pixelSize: 10
                            horizontalAlignment: Text.AlignHCenter
                            elide: Text.ElideRight
                            renderType: Text.NativeRendering
                        }

                        MouseArea {
                            anchors.fill: parent
                            drag.target: monitorCard
                            cursorShape: drag.active ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                            onPressed: pane.selectedIndex = monitorCard.index
                            onReleased: pane.moveDisplay(monitorCard.index,
                                (monitorCard.x - pane.canvasPadding - pane.canvasOffsetX())
                                    / monitorCard.factor + pane.minimumX(),
                                (monitorCard.y - pane.canvasPadding - pane.canvasOffsetY())
                                    / monitorCard.factor + pane.minimumY())
                        }
                    }
                }
            }

            // Mirrors are not on the canvas above, so this is the only handle
            // they have: it names what each one copies and selects it for the
            // settings below.
            Flow {
                Layout.fillWidth: true
                Layout.topMargin: 2
                spacing: 7
                visible: pane.mirrorCount() > 0

                Repeater {
                    model: pane.layout

                    ContinuousRectangle {
                        id: mirrorChip
                        required property int index
                        required property var modelData
                        visible: !pane.inArrangement(index)
                        width: chipLabel.implicitWidth + 20
                        height: 24
                        radius: Theme.controlRadius
                        color: index === pane.selectedIndex ? Theme.accentSoft : Theme.surface
                        borderWidth: 1
                        borderColor: index === pane.selectedIndex ? Theme.accent : Theme.border

                        Text {
                            id: chipLabel
                            anchors.centerIn: parent
                            text: mirrorChip.modelData.output + " → "
                                + pane.mirrorSource(mirrorChip.index)
                            color: mirrorChip.index === pane.selectedIndex
                                ? Theme.accentText : Theme.textMuted
                            font.family: Theme.sans
                            font.pixelSize: 10
                            renderType: Text.NativeRendering
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: pane.selectedIndex = mirrorChip.index
                        }
                    }
                }
            }

            Text {
                Layout.fillWidth: true
                text: pane.mirrorCount() > 0
                    ? "Drag displays to match their physical arrangement. Mirrors have no place of their own."
                    : "Drag displays to match their physical arrangement."
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 10
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                renderType: Text.NativeRendering
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Text {
                Layout.fillWidth: true
                text: pane.dirty ? "Changes pending" : pane.layout.length
                    + (pane.layout.length === 1 ? " display" : " displays")
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                renderType: Text.NativeRendering
            }
            SettingsButton {
                text: "Revert"
                enabled: pane.dirty
                onClicked: { pane.dirty = false; pane.reload(); }
            }
            SettingsButton {
                text: "Apply"
                prominent: true
                enabled: pane.dirty && pane.enabledCount() > 0 && !pane.controller.displayPending
                onClicked: pane.testLayout()
            }
        }

        SettingsGroup {
            visible: pane.layout.length > 0
            title: pane.layout[pane.selectedIndex]
                ? (pane.layout[pane.selectedIndex].model || pane.layout[pane.selectedIndex].output).toUpperCase()
                : "DISPLAY"

            SettingsRow {
                title: "Use This Display"
                SettingsSwitch {
                    checked: pane.selected("enabled", true) !== false
                    enabled: checked ? pane.enabledCount() > 1 : true
                    onToggled: value => pane.updateDisplay(pane.selectedIndex, "enabled", value)
                }
            }

            SettingsRow {
                title: "Primary Display"
                SettingsSwitch {
                    checked: pane.selected("primary", false)
                    onToggled: value => { if (value) pane.updateDisplay(pane.selectedIndex, "primary", true); }
                }
            }

            SettingsRow {
                id: mirrorRow
                title: "Mirror"
                description: pane.mirrorNote(pane.selectedIndex)
                // Only the sources Hyprland will honour are ever listed, so the
                // combo cannot express a chain, a cycle, or a display copying
                // itself. The primary is left out because dropping it out of
                // the arrangement would leave nothing showing the desktop.
                readonly property var sources: pane.selected("primary", false)
                    ? [] : pane.mirrorSources(pane.selectedIndex)
                rowEnabled: sources.length > 0 && pane.selected("enabled", true) !== false

                SettingsCombo {
                    enabled: mirrorRow.rowEnabled
                    model: ["Extend"].concat(mirrorRow.sources.map(output => "Mirror " + output))
                    currentIndex: Math.max(0, mirrorRow.sources.indexOf(
                        pane.mirrorSource(pane.selectedIndex)) + 1)
                    onActivated: index => pane.updateDisplay(pane.selectedIndex, "mirror",
                        index === 0 ? "" : mirrorRow.sources[index - 1])
                }
            }

            SettingsRow {
                title: "Resolution & Refresh Rate"
                SettingsCombo {
                    model: pane.selected("modes", [])
                    currentIndex: Math.max(0, model.indexOf(pane.selected("mode", "")))
                    onActivated: index => pane.updateDisplay(pane.selectedIndex, "mode", model[index])
                }
            }

            SettingsRow {
                title: "Scale"
                SettingsCombo {
                    property var scales: [1, 1.25, 1.5, 1.666667, 2]
                    model: ["100%", "125%", "150%", "167%", "200%"]
                    currentIndex: {
                        const value = Number(pane.selected("scale", 1));
                        let best = 0;
                        for (let i = 1; i < scales.length; ++i)
                            if (Math.abs(scales[i] - value) < Math.abs(scales[best] - value)) best = i;
                        return best;
                    }
                    onActivated: index => pane.updateDisplay(pane.selectedIndex, "scale", scales[index])
                }
            }

            SettingsRow {
                title: "Rotation"
                SettingsCombo {
                    model: ["Normal", "90°", "180°", "270°"]
                    currentIndex: Math.max(0, [0, 1, 2, 3].indexOf(
                        Number(pane.selected("transform", 0))))
                    onActivated: index => pane.updateDisplay(pane.selectedIndex, "transform", index)
                }
            }

            SettingsRow {
                id: vrrRow
                title: "Variable Refresh Rate"
                // Not read back from the compositor: hyprctl reports whether
                // adaptive sync is on right now, not the mode that was asked
                // for, so the index has to come from the saved layout.
                readonly property int mode: Math.max(0, pane.vrrModes.indexOf(
                    Number(pane.selected("vrr", -1))))
                description: pane.vrrNotes[mode]
                rowEnabled: pane.selected("enabled", true) !== false

                SettingsCombo {
                    enabled: vrrRow.rowEnabled
                    model: ["Follow System", "Off", "Always", "Fullscreen", "Games & Video"]
                    currentIndex: vrrRow.mode
                    onActivated: index => pane.updateDisplay(
                        pane.selectedIndex, "vrr", pane.vrrModes[index])
                }
            }
        }

        Item { Layout.preferredHeight: 20 }
    }
}
