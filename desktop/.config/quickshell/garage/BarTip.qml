import QtQuick

// The bar's tooltip bubble.
//
// Declared inside any bar module as a child -- it positions itself under its owner
// and clamps along the window's long axis. It opens inward from whichever edge
// owns the bar.
Item {
    id: tip

    property string text: ""
    property string edge: BarState.position
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
            if (tip.edge === "left")
                return tip.owner.width + 4;
            if (tip.edge === "right")
                return -width - 4;
            const scene = tip.owner.mapToItem(null, tip.owner.width / 2, 0);
            const raw = scene.x - width / 2;
            const window_ = tip.owner.Window.window;
            const rightLimit = window_ ? window_.width - width - 4 : raw;
            const clamped = Math.max(4, Math.min(raw, rightLimit));
            return tip.mapFromItem(null, clamped, 0).x;
        }
        y: {
            if (tip.edge === "top")
                return tip.owner.height + 4;
            if (tip.edge === "bottom")
                return -height - 4;
            const scene = tip.owner.mapToItem(null, 0, tip.owner.height / 2);
            const raw = scene.y - height / 2;
            const window_ = tip.owner.Window.window;
            const bottomLimit = window_ ? window_.height - height - 4 : raw;
            const clamped = Math.max(4, Math.min(raw, bottomLimit));
            return tip.mapFromItem(null, 0, clamped).y;
        }

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
