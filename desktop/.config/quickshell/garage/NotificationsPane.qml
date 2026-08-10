import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "NOTIFICATION CENTER"

            SettingsRow {
                title: "Do Not Disturb"
                description: "Silence banners while keeping notifications in Notification Center."
                SettingsSwitch {
                    checked: NotificationDaemon.dnd
                    onToggled: value => NotificationDaemon.setDnd(value)
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Notification History"
                description: "Remove all notifications currently stored."
                SettingsButton {
                    text: "Clear All"
                    onClicked: NotificationDaemon.clearAll()
                }
            }
        }

        Text {
            Layout.fillWidth: true
            text: "Notifications are handled by the Garage shell."
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 11
            horizontalAlignment: Text.AlignHCenter
            renderType: Text.NativeRendering
        }

        Item { Layout.preferredHeight: 20 }
    }
}
