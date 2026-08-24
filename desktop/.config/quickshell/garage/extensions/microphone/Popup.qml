pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

// The source list behind the privacy indicator. BarContext already normalises
// Pulse monitor sources away, so every row here represents a real input source
// whose state is RUNNING.
Item {
    id: microphonePopup

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    readonly property var context: services.context
    readonly property var theme: bar.theme
    readonly property bool available: context.micAvailable
    readonly property bool recording: available && context.micRecording
    readonly property var sources: {
        if (!Array.isArray(context.micDescriptions))
            return [];
        return context.micDescriptions.map(description => String(description).trim())
            .filter(description => description !== "");
    }
    readonly property string detail: {
        if (!available) {
            const error = String(context.probeError || "").trim();
            return error !== "" ? error : "The microphone probe is unavailable.";
        }
        if (!recording)
            return "No input source is currently recording.";
        if (sources.length === 0)
            return "An unidentified input source is recording.";
        return sources.length === 1
            ? "1 active recording source" : sources.length + " active recording sources";
    }

    implicitWidth: 304
    implicitHeight: content.implicitHeight + 36

    ColumnLayout {
        id: content

        anchors.fill: parent
        anchors.margins: 18
        spacing: 12

        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Rectangle {
                Layout.preferredWidth: 9
                Layout.preferredHeight: 9
                radius: 5
                color: microphonePopup.recording
                    ? microphonePopup.theme.accentPalette.red
                    : microphonePopup.available
                        ? microphonePopup.theme.textDisabled
                        : "transparent"
                border.width: microphonePopup.available ? 0 : 1
                border.color: microphonePopup.theme.textDisabled
            }

            Text {
                Layout.fillWidth: true
                text: "Microphone"
                color: microphonePopup.theme.text
                font.family: microphonePopup.theme.sans
                font.pixelSize: 15
                font.weight: Font.DemiBold
                renderType: Text.NativeRendering
            }

            Text {
                text: !microphonePopup.available ? "Unavailable"
                    : microphonePopup.recording ? "Recording" : "Idle"
                color: microphonePopup.recording
                    ? microphonePopup.theme.accentPalette.red
                    : microphonePopup.theme.textMuted
                font.family: microphonePopup.theme.sans
                font.pixelSize: 12
                font.weight: Font.DemiBold
                renderType: Text.NativeRendering
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: microphonePopup.theme.border
        }

        Text {
            Layout.fillWidth: true
            text: microphonePopup.detail
            color: microphonePopup.theme.textMuted
            font.family: microphonePopup.theme.sans
            font.pixelSize: 12
            wrapMode: Text.Wrap
            renderType: Text.NativeRendering
        }

        Repeater {
            model: microphonePopup.recording ? microphonePopup.sources : []

            RowLayout {
                id: sourceRow

                required property string modelData
                Layout.fillWidth: true
                spacing: 10

                Rectangle {
                    Layout.preferredWidth: 6
                    Layout.preferredHeight: 6
                    radius: 3
                    color: microphonePopup.theme.accentPalette.red
                }

                Text {
                    Layout.fillWidth: true
                    text: sourceRow.modelData
                    color: microphonePopup.theme.text
                    font.family: microphonePopup.theme.sans
                    font.pixelSize: 13
                    elide: Text.ElideRight
                    renderType: Text.NativeRendering
                }
            }
        }
    }
}
