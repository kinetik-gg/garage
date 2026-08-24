import QtQuick
import Qt5Compat.GraphicalEffects
import "../.." as Garage

// A persistent privacy indicator: idle is deliberately visible, while active
// recording turns the icon and its badge red so capture cannot look like the
// absence of a status module.
Item {
    id: microphoneWidget

    required property var bar
    required property var services
    required property var manifest
    property var probe: null

    readonly property var context: services.context
    readonly property var theme: bar.theme
    readonly property bool available: context.micAvailable
    readonly property bool recording: available && context.micRecording

    implicitWidth: 24 + bar.spacing.icon
    implicitHeight: 24

    Rectangle {
        anchors.fill: parent
        radius: 8
        color: pointer.pressed ? Qt.alpha(microphoneWidget.theme.text, 0.22)
            : microphoneWidget.recording
                ? Qt.alpha(microphoneWidget.theme.accentPalette.red, 0.16)
                : pointer.containsMouse
                    ? Qt.alpha(microphoneWidget.theme.text, 0.12)
                    : "transparent"

        Behavior on color {
            ColorAnimation { duration: microphoneWidget.theme.reduceMotion ? 0 : 130 }
        }
    }

    Item {
        anchors.centerIn: parent
        width: 16
        height: 16

        Image {
            id: microphoneSource

            anchors.fill: parent
            source: Garage.GaragePaths.shellDir + "/icons/microphone.svg"
            sourceSize.width: 32
            sourceSize.height: 32
            fillMode: Image.PreserveAspectFit
            smooth: true
            visible: false
        }

        ColorOverlay {
            anchors.fill: microphoneSource
            source: microphoneSource
            color: microphoneWidget.recording
                ? microphoneWidget.theme.accentPalette.red
                : microphoneWidget.available
                    ? microphoneWidget.theme.textMuted
                    : microphoneWidget.theme.textDisabled
            cached: true

            Behavior on color {
                ColorAnimation { duration: microphoneWidget.theme.reduceMotion ? 0 : 130 }
            }
        }

        Rectangle {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.rightMargin: -3
            anchors.topMargin: -3
            width: 7
            height: 7
            radius: 4
            visible: microphoneWidget.recording
            color: microphoneWidget.theme.accentPalette.red
            border.width: 1
            border.color: microphoneWidget.theme.bodyBase
        }
    }

    MouseArea {
        id: pointer

        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton
        onClicked: microphoneWidget.bar.openPopup(
            Qt.resolvedUrl("Popup.qml"), {})
    }

    Garage.BarTip {
        owner: microphoneWidget
        text: {
            if (!microphoneWidget.available)
                return "Microphone status unavailable";
            if (!microphoneWidget.recording)
                return "Microphone idle";
            const count = microphoneWidget.context.micDescriptions.length;
            return count > 0
                ? "Microphone recording\n" + microphoneWidget.context.micDescriptions.join("\n")
                : "Microphone recording";
        }
        opacity: pointer.containsMouse ? 1 : 0
    }
}
