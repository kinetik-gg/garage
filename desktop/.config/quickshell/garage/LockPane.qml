import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property var timeoutValues: [0, 60, 300, 600, 900, 1800, 3600]
    readonly property var timeoutLabels: ["Never", "1 minute", "5 minutes", "10 minutes", "15 minutes", "30 minutes", "1 hour"]

    function timeoutIndex(value) {
        const index = timeoutValues.indexOf(Number(value));
        return index < 0 ? 0 : index;
    }

    readonly property bool unsafeOrder: {
        const lock = controller.preference("lock", "lock_timeout", 600);
        const off = controller.preference("lock", "display_off_timeout", 900);
        const suspend = controller.preference("lock", "suspend_timeout", 0);
        return lock > 0 && ((off > 0 && off < lock) || (suspend > 0 && suspend < lock));
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "IDLE BEHAVIOR"

            SettingsRow {
                title: "Lock Screen"
                description: "Require authentication after the system is idle."
                SettingsCombo {
                    model: pane.timeoutLabels
                    currentIndex: pane.timeoutIndex(pane.controller.preference("lock", "lock_timeout", 600))
                    onActivated: index => pane.controller.setPreference(
                        "lock", "lock_timeout", pane.timeoutValues[index])
                }
            }

            SettingsRow {
                title: "Turn Displays Off"
                SettingsCombo {
                    model: pane.timeoutLabels
                    currentIndex: pane.timeoutIndex(pane.controller.preference("lock", "display_off_timeout", 900))
                    onActivated: index => pane.controller.setPreference(
                        "lock", "display_off_timeout", pane.timeoutValues[index])
                }
            }

            SettingsRow {
                title: "Suspend"
                SettingsCombo {
                    model: pane.timeoutLabels
                    currentIndex: pane.timeoutIndex(pane.controller.preference("lock", "suspend_timeout", 0))
                    onActivated: index => pane.controller.setPreference(
                        "lock", "suspend_timeout", pane.timeoutValues[index])
                }
            }
        }

        ContinuousRectangle {
            Layout.fillWidth: true
            visible: pane.unsafeOrder
            implicitHeight: warningText.implicitHeight + 24
            color: Theme.hoverStrong
            borderWidth: 1
            borderColor: Theme.border

            Text {
                id: warningText
                anchors.fill: parent
                anchors.margins: 12
                text: "Displays or suspension are scheduled before the lock screen. The session may remain visible briefly."
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                wrapMode: Text.WordWrap
                renderType: Text.NativeRendering
            }
        }

        SettingsButton {
            Layout.alignment: Qt.AlignHCenter
            text: "Lock Now"
            prominent: true
            onClicked: pane.controller.action("lock.now")
        }

        Item { Layout.preferredHeight: 20 }
    }
}
