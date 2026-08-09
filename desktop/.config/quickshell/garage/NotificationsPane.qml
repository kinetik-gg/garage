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
                rowEnabled: pane.controller.snapshot.notifications.available
                SettingsSwitch {
                    checked: pane.controller.snapshot.notifications.dnd
                    enabled: pane.controller.snapshot.notifications.available
                    onToggled: value => pane.controller.action("notifications.dnd", value)
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Notification History"
                description: "Remove all notifications currently stored by SwayNC."
                rowEnabled: pane.controller.snapshot.notifications.available
                SettingsButton {
                    text: "Clear All"
                    enabled: pane.controller.snapshot.notifications.available
                    onClicked: pane.controller.action("notifications.clear")
                }
            }
        }

        Text {
            Layout.fillWidth: true
            text: pane.controller.snapshot.notifications.available
                ? "Notifications are managed by SwayNC."
                : "The notification service is unavailable."
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 11
            horizontalAlignment: Text.AlignHCenter
            renderType: Text.NativeRendering
        }

        Item { Layout.preferredHeight: 20 }
    }
}
