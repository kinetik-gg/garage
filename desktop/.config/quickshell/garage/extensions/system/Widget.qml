import QtQuick
import Qt5Compat.GraphicalEffects
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

    implicitWidth: 24 + bar.spacing.icon
    implicitHeight: 24

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

    Item {
        id: monitorIcon

        anchors.centerIn: parent
        width: 16
        height: 16

        Image {
            id: monitorSource

            anchors.fill: parent
            source: Garage.GaragePaths.shellDir + "/icons/monitor.svg"
            sourceSize.width: 32
            sourceSize.height: 32
            fillMode: Image.PreserveAspectFit
            smooth: true
            visible: false
        }

        ColorOverlay {
            anchors.fill: monitorSource
            source: monitorSource
            color: systemWidget.theme.text
            cached: true
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
