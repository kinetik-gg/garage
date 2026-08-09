import Quickshell
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

ContinuousRectangle {
    id: row

    property string kind: "app"
    property string title: ""
    property string subtitle: ""
    // Icon-theme name for apps; the built-in glyphs cover the other kinds.
    property string iconName: ""
    property bool selected: false

    signal activated()
    signal hovered()

    // A list row is a control, not a panel: tighter than the surface it sits on.
    radius: Theme.controlRadius
    color: selected ? Theme.accent : (pointer.containsMouse ? Theme.hover : "transparent")

    readonly property bool usesGlyph: kind !== "app" && iconName === ""
    readonly property string glyph: kind === "calc"
        ? "icons/calculator.svg" : "icons/globe.svg"

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: 12

        Item {
            Layout.preferredWidth: 30
            Layout.preferredHeight: 30

            // App icons come from the icon theme, so they are already coloured
            // and must not be overlaid.
            Image {
                id: themeIcon
                anchors.fill: parent
                visible: !row.usesGlyph
                source: row.iconName === "" ? "" : Quickshell.iconPath(row.iconName, true)
                sourceSize.width: 60
                sourceSize.height: 60
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
            }

            Image {
                id: builtinGlyph
                anchors.centerIn: parent
                width: 22
                height: 22
                visible: false
                source: row.usesGlyph ? row.glyph : ""
                sourceSize.width: 44
                sourceSize.height: 44
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
            }

            ColorOverlay {
                anchors.fill: builtinGlyph
                source: builtinGlyph
                visible: row.usesGlyph
                color: row.selected ? Theme.accentText : Theme.text
                cached: true
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 1

            Text {
                Layout.fillWidth: true
                text: row.title
                color: row.selected ? Theme.accentText : Theme.text
                font.family: Theme.sans
                font.pixelSize: 14
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }

            Text {
                Layout.fillWidth: true
                text: row.subtitle
                visible: row.subtitle !== ""
                color: row.selected ? Theme.accentText : Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }
        }
    }

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onEntered: row.hovered()
        onClicked: row.activated()
    }
}
