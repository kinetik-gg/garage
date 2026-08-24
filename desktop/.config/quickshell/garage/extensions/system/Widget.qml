import QtQuick
import "../.." as Garage

// One monitor glyph and one badge. The badge is deliberately reserved for
// privacy and connectivity: microphone recording wins over an SMB shortfall;
// telemetry, AI and containers remain detail in the panel and tooltip.
Item {
    id: systemWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    readonly property var context: services.context
    readonly property var theme: bar.theme
    readonly property bool micRecording: context.micAvailable && context.micRecording
    readonly property bool smbShort: context.smbAvailable
        && context.smbConnected < context.smbExpected

    implicitWidth: 18 + bar.spacing.icon * 2
    implicitHeight: Math.max(24, 16 + 8)

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: pointer.pressed ? Qt.alpha(systemWidget.theme.text, 0.22)
            : pointer.containsMouse ? Qt.alpha(systemWidget.theme.text, 0.12)
            : "transparent"

        Behavior on color {
            ColorAnimation { duration: systemWidget.theme.reduceMotion ? 0 : 130 }
        }
    }

    // A small code-native monitor outline so it follows the live foreground
    // role in both schemes instead of baking a colour into an SVG.
    Item {
        id: monitorIcon

        anchors.centerIn: parent
        width: 18
        height: 16

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: parent.top
            width: 18
            height: 12
            radius: 2
            color: "transparent"
            border.width: 1.5
            border.color: systemWidget.theme.text
        }

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            width: 8
            height: 1.5
            radius: 1
            color: systemWidget.theme.text
        }

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 1
            width: 1.5
            height: 4
            color: systemWidget.theme.text
        }

        Rectangle {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.rightMargin: -2
            anchors.topMargin: -2
            width: 7
            height: 7
            radius: 4
            visible: systemWidget.micRecording || systemWidget.smbShort
            color: systemWidget.micRecording
                ? systemWidget.theme.accent
                : systemWidget.theme.accentPalette.red
            border.width: 1
            border.color: systemWidget.theme.bodyBase
        }
    }

    MouseArea {
        id: pointer

        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton
        onClicked: systemWidget.bar.openSurface("system", systemWidget)
    }

    Garage.BarTip {
        owner: systemWidget
        text: {
            const lines = [];
            if (systemWidget.micRecording)
                lines.push("Microphone in use");
            if (systemWidget.smbShort)
                lines.push("SMB " + systemWidget.context.smbConnected + " / "
                    + systemWidget.context.smbExpected);
            if (systemWidget.context.containerCount > 0)
                lines.push(systemWidget.context.containerCount + " containers running");
            if (systemWidget.context.aiAvailable && systemWidget.context.aiGlyph !== "")
                lines.push("AI " + systemWidget.context.aiGlyph
                    + (systemWidget.context.aiStale ? " (cached)" : ""));
            return lines.length > 0 ? lines.join("\n") : "System";
        }
        opacity: pointer.containsMouse ? 1 : 0
    }
}
