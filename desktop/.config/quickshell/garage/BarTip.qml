import QtQuick

// The bar's tooltip bubble.
//
// Declared inside any bar module as a child -- it positions itself under its owner
// and clamps to the window's edges -- and driven by the module's own hover state:
// hovered changed to true calls show(), to false hides it. Opens downward, the way
// every bar tooltip has always read.
Item {
    id: tip

    property string text: ""
    // The module the tip describes; centred under this and shown below it.
    property Item owner: parent

    anchors.top: parent.top
    width: 1
    height: 1
    visible: opacity > 0
    opacity: 0
    z: 1000

    function show() {
        opacity = 1;
    }

    function hide() {
        opacity = 0;
    }

    Behavior on opacity {
        NumberAnimation { duration: Theme.reduceMotion ? 0 : 110 }
    }

    Rectangle {
        id: bubble

        readonly property int gutter: BarState.scaled("tooltip")

        x: {
            const centre = tip.owner.width / 2;
            const raw = centre - width / 2 - tip.owner.x + tip.x;
            const window_ = tip.owner.Window.window;
            const rightLimit = window_ ? window_.width - width - 4 : raw;
            return Math.max(4, Math.min(raw, rightLimit)) - tip.x;
        }
        y: tip.owner.height + 4 - tip.y

        width: label.implicitWidth + gutter * 2
        height: label.implicitHeight + Math.round(gutter * 1.5)

        color: Theme.bodyRaised
        radius: 10
        border.color: Theme.frameOuter
        border.width: 1

        Text {
            id: label

            anchors.centerIn: parent
            text: tip.text
            color: Theme.text
            font.family: Theme.sans
            font.pixelSize: 13
            font.weight: Font.DemiBold
            renderType: Text.NativeRendering
        }
    }
}
