import QtQuick
import QtQuick.Window
import QtQuick.Controls as Controls

Controls.ComboBox {
    id: combo
    property var values: model
    property real maxPopupHeight: 220
    implicitWidth: 190
    implicitHeight: 32
    font.family: Theme.sans
    font.pixelSize: 12

    HoverHandler {
        cursorShape: combo.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
    }

    contentItem: Text {
        leftPadding: 11
        rightPadding: 28
        text: combo.displayText
        color: combo.enabled ? Theme.text : Theme.textDisabled
        font: combo.font
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        renderType: Text.NativeRendering
    }

    indicator: Text {
        x: combo.width - width - 10
        anchors.verticalCenter: parent.verticalCenter
        text: "⌄"
        color: Theme.textMuted
        font.family: Theme.sans
        font.pixelSize: 13
        renderType: Text.NativeRendering
    }

    background: ContinuousRectangle {
        radius: Theme.controlRadius
        color: combo.hovered ? Theme.hoverStrong : Theme.hover
        borderWidth: 1
        borderColor: Theme.border
    }

    delegate: Controls.ItemDelegate {
        id: option
        width: ListView.view ? ListView.view.width : combo.width - 8
        implicitHeight: 32
        hoverEnabled: true

        HoverHandler {
            enabled: option.enabled
            cursorShape: Qt.PointingHandCursor
        }

        contentItem: Text {
            text: modelData
            color: option.hovered || option.highlighted ? Theme.accentText : Theme.text
            font: combo.font
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
            renderType: Text.NativeRendering
        }
        background: Item {
            ContinuousRectangle {
                anchors.fill: parent
                anchors.margins: 1
                radius: Theme.insetRadius(Theme.controlRadius, 1)
                color: option.hovered || option.highlighted
                    ? Theme.accent : Theme.surfaceRaised
            }
        }
    }

    popup: Controls.Popup {
        id: popup
        readonly property real listHeight: Math.min(optionList.contentHeight, combo.maxPopupHeight)
        y: {
            const sceneY = combo.mapToItem(null, 0, 0).y;
            const host = combo.Window.window;
            const roomBelow = host ? host.height - sceneY - combo.height : implicitHeight + 8;
            return roomBelow >= implicitHeight + 8 ? combo.height + 4 : -implicitHeight - 4;
        }
        width: combo.width
        implicitHeight: listHeight + topPadding + bottomPadding
        padding: 4
        contentItem: ListView {
            id: optionList
            clip: true
            implicitHeight: popup.listHeight
            model: combo.popup.visible ? combo.delegateModel : null
            currentIndex: combo.highlightedIndex
            boundsBehavior: Flickable.StopAtBounds

            Controls.ScrollBar.vertical: Controls.ScrollBar {
                policy: optionList.contentHeight > optionList.height
                    ? Controls.ScrollBar.AsNeeded : Controls.ScrollBar.AlwaysOff
            }
        }
        background: ContinuousRectangle {
            color: Theme.surfaceRaised
            borderWidth: 1
            borderColor: Theme.border
        }
    }
}
