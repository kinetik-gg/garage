import QtQuick
import Quickshell
import Quickshell.Widgets

// The system tray, one StatusNotifierItem per icon. Left-click runs the item's
// primary action, right-click its secondary, and the wheel is forwarded with the
// sign the SNI spec expects.
//
// Menu-only items are activated like any other here: presenting the item's
// DBusMenu needs a menu surface of its own, which is a deliberate follow-up --
// every tray resident on this desktop answers activate() today.
Row {
    id: tray

    spacing: BarState.scaled("tray")

    Repeater {
        model: Quickshell.tray?.items ?? []

        delegate: Item {
            id: holder

            required property var modelData

            width: 19
            height: 19

            IconImage {
                anchors.fill: parent
                source: holder.modelData ? String(holder.modelData.icon) : ""
                asynchronous: true
            }

            MouseArea {
                id: area

                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                cursorShape: Qt.PointingHandCursor
                onClicked: mouse => {
                    if (!holder.modelData)
                        return;
                    if (mouse.button === Qt.RightButton && holder.modelData.secondaryActivate)
                        holder.modelData.secondaryActivate();
                    else if (holder.modelData.activate)
                        holder.modelData.activate();
                }
                // The spec's scroll deltas are negative-down; QML's wheel is
                // positive-down, so the sign flips once here.
                WheelHandler {
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    onWheel: event => {
                        const item = holder.modelData;
                        if (item && item.scroll)
                            item.scroll(-event.angleDelta.y / 120, false);
                        event.accepted = true;
                    }
                }
            }
        }
    }
}
