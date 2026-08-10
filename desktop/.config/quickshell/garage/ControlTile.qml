import QtQuick
import Qt5Compat.GraphicalEffects

// One cell of the control centre grid: a glyph in a well, a title, and a line
// of state under it.
//
// The on/off state is drawn on the well rather than on the tile body. A grid of
// tiles that each filled with the accent when switched on reads as a patchwork
// of unrelated cards; lighting only the glyph keeps the grid one surface with
// some of its icons lit, which is what the tile is imitating. The body is left
// to carry hover, so the whole tile still reads as one button.
ContinuousRectangle {
    id: tile

    property string iconSource: ""
    property string title: ""
    property string subtitle: ""
    property bool active: false

    signal toggled()

    // Half of a 390 px panel less its 12 px margins and the 8 px gutter between
    // the columns. A preferred width in the grid rather than a hard one, so the
    // cell still stretches if the panel width is ever changed.
    implicitWidth: 179
    implicitHeight: 62

    // The window radius rather than the control one: a tile is a surface with
    // its own contents, the same as the icon wells in the session dialog, not a
    // button-sized control.
    radius: Theme.cornerRadius
    color: pointer.containsMouse ? Theme.hoverStrong : Theme.hover
    borderWidth: 1
    borderColor: Theme.frameInner
    opacity: tile.enabled ? 1 : 0.45

    ContinuousRectangle {
        id: well
        anchors.left: parent.left
        anchors.leftMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: 34
        implicitHeight: 34
        radius: Theme.controlRadius
        color: tile.active ? Theme.accent : Theme.iconWell

        Image {
            id: glyph
            anchors.centerIn: parent
            width: 18
            height: 18
            source: tile.iconSource
            sourceSize.width: 36
            sourceSize.height: 36
            fillMode: Image.PreserveAspectFit
            smooth: true
            antialiasing: true
            mipmap: true
            // The svgs ship their own colour, so the glyph is drawn through an
            // overlay and this copy only supplies the shape.
            visible: false
        }

        ColorOverlay {
            anchors.fill: glyph
            source: glyph
            color: tile.active ? Theme.accentText : Theme.iconWellGlyph
            cached: true
        }
    }

    Column {
        anchors.left: well.right
        anchors.leftMargin: 10
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2

        Text {
            width: parent.width
            text: tile.title
            color: Theme.text
            font.family: Theme.sans
            font.pixelSize: 12
            font.weight: Font.DemiBold
            elide: Text.ElideRight
            renderType: Text.NativeRendering
        }

        Text {
            width: parent.width
            visible: tile.subtitle !== ""
            text: tile.subtitle
            color: Theme.textMuted
            font.family: Theme.sans
            font.pixelSize: 11
            elide: Text.ElideRight
            renderType: Text.NativeRendering
        }
    }

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: tile.toggled()
    }
}
