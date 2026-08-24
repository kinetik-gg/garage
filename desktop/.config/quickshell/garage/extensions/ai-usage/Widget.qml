import QtQuick
import "../.." as Garage

// The five-minute bar summary is shell-owned and shared across outputs. Detail
// opens through the shell surface table so the bar click and legacy ai-usage IPC
// route are mutually exclusive with every other transient panel.
Garage.BarIconButton {
    id: aiWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    readonly property var context: services.context
    readonly property var theme: bar.theme

    glyph: context.aiGlyph
    // garage-ai-usage emits Phosphor's sparkle. Use the same fixed 16px SVG
    // footprint as the other extension icons when that mark is unavailable.
    glyphFamily: "Phosphor"
    iconSource: context.aiGlyph === ""
        ? Garage.GaragePaths.shellDir + "/icons/cpu.svg" : ""
    glyphSize: 16

    onActivated: bar.openSurface("ai-usage", aiWidget)

    Rectangle {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: 3
        anchors.bottomMargin: 3
        width: 5
        height: 5
        radius: 3
        visible: aiWidget.context.aiStale || !aiWidget.context.aiAvailable
        color: aiWidget.context.aiStale
            ? aiWidget.theme.accentPalette.orange
            : aiWidget.theme.textDisabled
        border.width: 1
        border.color: aiWidget.theme.bodyBase
    }

    HoverHandler { id: hover }

    Garage.BarTip {
        owner: aiWidget
        text: {
            if (aiWidget.context.aiTip !== "")
                return aiWidget.context.aiTip;
            if (aiWidget.context.aiStale)
                return "AI usage unavailable · cached detail may be available";
            return "AI usage unavailable · install Tokscale";
        }
        opacity: hover.hovered ? 1 : 0
    }
}
