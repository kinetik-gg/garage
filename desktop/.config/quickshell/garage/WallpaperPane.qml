import Quickshell
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts
import Qt.labs.folderlistmodel

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // The half of the schema being edited. Held on the controller because the
    // sidebar drives Loader.setSource, which destroys and rebuilds this pane on
    // every visit -- a property here would snap back the moment you looked away.
    readonly property string scheme: pane.controller.wallpaperScheme
    readonly property var schemes: ["light", "dark"]
    readonly property var schemeTitles: ({ light: "Light", dark: "Dark" })

    // Solid desktop colours. Muted rather than saturated: this is the surface
    // every window sits on, and the material's tint reads off it.
    readonly property var wallpaperColors: [
        "#1c1c1e", "#4a4a4f", "#1b2a4a", "#14566b",
        "#1f4d38", "#7a3b2e", "#4a2c56", "#d6cbb8"
    ]

    function pref(scheme, suffix, fallback) {
        return pane.controller.preference("appearance", "wallpaper_" + scheme + suffix, fallback);
    }
    function isColor(scheme) {
        return pane.pref(scheme, "_source", "image") === "color";
    }

    // Image.source is a url, and a bare path with a space in it does not survive
    // the string conversion. Every shipped wallpaper is named "Title - Author",
    // so this is the normal case rather than the exotic one.
    function fileUrl(path) {
        return path ? "file://" + encodeURI(path) : "";
    }
    function localPath(url) {
        return decodeURIComponent(url.toString().replace(/^file:\/\//, ""));
    }

    // Picking a picture also has to leave the source on "image", or the desktop
    // would keep the colour and the choice would look ignored. Written second:
    // the helper serialises both under its preferences lock, so the switch
    // always lands on the new path.
    function chooseImage(path) {
        pane.controller.setPreference("appearance", "wallpaper_" + pane.scheme, path);
        if (pane.isColor(pane.scheme))
            pane.controller.setPreference(
                "appearance", "wallpaper_" + pane.scheme + "_source", "image");
    }

    // The selection dot sits on the swatch, not on the pane, so it has to
    // contrast with the swatch rather than follow the theme foreground.
    function dotOver(swatch) {
        const c = Qt.color(swatch);
        return c.r * 0.299 + c.g * 0.587 + c.b * 0.114 > 0.55 ? "#1c1c1e" : "#f5f5f7";
    }

    // The wallpapers shipped with the dotfiles, sorted into an appearance each.
    FolderListModel {
        id: shipped
        folder: "file://" + Quickshell.env("HOME") + "/Wallpaper/"
            + pane.schemeTitles[pane.scheme]
        nameFilters: ["*.png", "*.jpg", "*.jpeg", "*.webp", "*.jxl"]
        showDirs: false
        showDotAndDotDot: false
        sortField: FolderListModel.Name
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "DESKTOP PICTURE"

            // Both halves are on screen at once so the split cannot be a
            // surprise: a user in dark mode has to see that light is a separate
            // picture before they wonder why theirs did not follow.
            RowLayout {
                Layout.fillWidth: true
                spacing: 14

                Repeater {
                    model: pane.schemes

                    ColumnLayout {
                        id: card
                        required property string modelData
                        readonly property bool editing: pane.scheme === card.modelData
                        readonly property bool live: Theme.scheme === card.modelData
                        Layout.fillWidth: true
                        // Equal halves. Without a shared preferred width the row
                        // sizes each card from its caption, and the one carrying
                        // the "On screen" badge takes the space from the other.
                        Layout.preferredWidth: 1
                        Layout.minimumWidth: 0
                        spacing: 7

                        ContinuousRectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: Math.round(width * 9 / 16)
                            color: pane.isColor(card.modelData)
                                ? pane.pref(card.modelData, "_color", "#1c1c1e")
                                : Theme.surface
                            borderWidth: card.editing ? 2 : 1
                            borderColor: card.editing ? Theme.accent : Theme.frameInner

                            Image {
                                id: preview
                                anchors.fill: parent
                                anchors.margins: 2
                                source: pane.isColor(card.modelData) ? ""
                                    : pane.fileUrl(pane.pref(card.modelData, "", ""))
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                                sourceSize.width: 640
                                smooth: true
                                visible: false
                            }

                            ContinuousRectangle {
                                id: previewMask
                                anchors.fill: preview
                                color: "white"
                                visible: false
                            }

                            OpacityMask {
                                anchors.fill: preview
                                visible: !pane.isColor(card.modelData)
                                source: preview
                                maskSource: previewMask
                                cached: true
                            }

                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: pane.controller.wallpaperScheme = card.modelData
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            spacing: 6

                            Text {
                                text: pane.schemeTitles[card.modelData]
                                color: card.editing ? Theme.text : Theme.textMuted
                                font.family: Theme.sans
                                font.pixelSize: 12
                                font.weight: card.editing ? Font.DemiBold : Font.Normal
                                renderType: Text.NativeRendering
                            }

                            // Which appearance the desktop is actually showing.
                            // Without it the pane cannot explain why editing one
                            // card changes the wallpaper and the other does not.
                            ContinuousRectangle {
                                visible: card.live
                                implicitWidth: liveLabel.implicitWidth + 12
                                implicitHeight: 16
                                radius: Theme.controlRadius
                                color: Theme.hoverStrong

                                Text {
                                    id: liveLabel
                                    anchors.centerIn: parent
                                    text: "On screen"
                                    color: Theme.textMuted
                                    font.family: Theme.sans
                                    font.pixelSize: 10
                                    renderType: Text.NativeRendering
                                }
                            }

                            Item { Layout.fillWidth: true }
                        }
                    }
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            // A page-level strip rather than an inline control, so the segmented
            // picker's row-sized implicit width has to be dropped for the layout
            // to stretch it.
            SettingsSegmented {
                Layout.fillWidth: true
                implicitWidth: 0
                implicitHeight: 34
                model: ["Default", "Custom", "Colors"]
                currentIndex: pane.controller.wallpaperTab
                onActivated: index => pane.controller.wallpaperTab = index
            }

            StackLayout {
                id: tabs
                Layout.fillWidth: true
                currentIndex: pane.controller.wallpaperTab
                // StackLayout takes the height of its tallest page, so the group
                // would stay as tall as the picture grid on every tab. Follow the
                // page actually being shown instead.
                Layout.preferredHeight: children[currentIndex]
                    ? children[currentIndex].implicitHeight : 0

                ColumnLayout {
                    spacing: 12

                    Text {
                        Layout.fillWidth: true
                        text: shipped.count > 0
                            ? "Pictures that ship with the desktop, chosen to read on "
                                + pane.schemeTitles[pane.scheme].toLowerCase() + "."
                            : "No pictures are installed in ~/Wallpaper/"
                                + pane.schemeTitles[pane.scheme] + "."
                        color: Theme.textMuted
                        font.family: Theme.sans
                        font.pixelSize: 11
                        wrapMode: Text.WordWrap
                        renderType: Text.NativeRendering
                    }

                    // Flow rather than a GridView: there are a handful of files,
                    // and a Flow has a real implicit height for the group to size
                    // itself from where a Flickable has none.
                    Flow {
                        Layout.fillWidth: true
                        spacing: 10

                        Repeater {
                            model: shipped

                            Column {
                                id: tile
                                required property url fileUrl
                                required property string fileBaseName
                                readonly property string path: pane.localPath(tile.fileUrl)
                                readonly property bool selected: !pane.isColor(pane.scheme)
                                    && pane.pref(pane.scheme, "", "") === tile.path
                                width: 128
                                spacing: 5

                                ContinuousRectangle {
                                    width: parent.width
                                    height: 72
                                    color: Theme.surface
                                    borderWidth: tile.selected ? 2 : 1
                                    borderColor: tile.selected ? Theme.accent
                                        : tilePointer.containsMouse ? Theme.border : Theme.frameInner

                                    Image {
                                        id: tileImage
                                        anchors.fill: parent
                                        anchors.margins: 2
                                        source: tile.fileUrl
                                        fillMode: Image.PreserveAspectCrop
                                        asynchronous: true
                                        sourceSize.width: 320
                                        smooth: true
                                        visible: false
                                    }
                                    ContinuousRectangle {
                                        id: tileMask
                                        anchors.fill: tileImage
                                        color: "white"
                                        visible: false
                                    }
                                    OpacityMask {
                                        anchors.fill: tileImage
                                        source: tileImage
                                        maskSource: tileMask
                                        cached: true
                                    }

                                    MouseArea {
                                        id: tilePointer
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: pane.chooseImage(tile.path)
                                    }
                                }

                                Text {
                                    width: parent.width
                                    text: tile.fileBaseName
                                    color: tile.selected ? Theme.text : Theme.textMuted
                                    font.family: Theme.sans
                                    font.pixelSize: 10
                                    horizontalAlignment: Text.AlignHCenter
                                    elide: Text.ElideRight
                                    renderType: Text.NativeRendering
                                }
                            }
                        }
                    }
                }

                RowLayout {
                    spacing: 18

                    ContinuousRectangle {
                        Layout.preferredWidth: 190
                        Layout.preferredHeight: 108
                        color: Theme.surface
                        borderWidth: 1
                        borderColor: Theme.frameInner

                        Image {
                            id: customImage
                            anchors.fill: parent
                            anchors.margins: 1
                            source: pane.isColor(pane.scheme) ? ""
                                : pane.fileUrl(pane.pref(pane.scheme, "", ""))
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            sourceSize.width: 640
                            smooth: true
                            visible: false
                        }
                        ContinuousRectangle {
                            id: customMask
                            anchors.fill: customImage
                            color: "white"
                            visible: false
                        }
                        OpacityMask {
                            anchors.fill: customImage
                            source: customImage
                            maskSource: customMask
                            cached: true
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 10

                        Text {
                            Layout.fillWidth: true
                            text: pane.pref(pane.scheme, "", "") || "No picture selected"
                            color: Theme.textMuted
                            font.family: Theme.sans
                            font.pixelSize: 11
                            elide: Text.ElideMiddle
                            renderType: Text.NativeRendering
                        }

                        SettingsButton {
                            text: "Choose Picture…"
                            // Switching the source here rather than waiting for
                            // the picker to return: the button reads as "go back
                            // to a picture", and it restores the stored image at
                            // once so dismissing the picker still leaves a
                            // coherent desktop.
                            onClicked: {
                                if (pane.isColor(pane.scheme))
                                    pane.controller.setPreference(
                                        "appearance", "wallpaper_" + pane.scheme + "_source",
                                        "image");
                                pane.controller.wallpaperPickerKey = "wallpaper_" + pane.scheme;
                                pane.controller.wallpaperPickerFolder =
                                    Quickshell.env("HOME") + "/Pictures";
                                pane.controller.wallpaperPickerOpen = true;
                            }
                        }

                        Item { Layout.fillHeight: true }
                    }
                }

                ColumnLayout {
                    spacing: 12

                    Text {
                        Layout.fillWidth: true
                        text: "Use a solid color instead of a picture."
                        color: Theme.textMuted
                        font.family: Theme.sans
                        font.pixelSize: 11
                        renderType: Text.NativeRendering
                    }

                    Flow {
                        Layout.fillWidth: true
                        spacing: 10

                        Repeater {
                            model: pane.wallpaperColors

                            Rectangle {
                                required property string modelData
                                width: 34
                                height: 34
                                radius: width / 2
                                color: modelData
                                border.width: pane.isColor(pane.scheme)
                                    && pane.pref(pane.scheme, "_color", "#1c1c1e") === modelData
                                    ? 2 : 0
                                border.color: Theme.text
                                antialiasing: true

                                Rectangle {
                                    anchors.centerIn: parent
                                    width: 8
                                    height: 8
                                    radius: width / 2
                                    visible: parent.border.width > 0
                                    color: pane.dotOver(modelData)
                                    antialiasing: true
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    // Colour first, then the source: the helper
                                    // serialises these under its preferences
                                    // lock, so the switch always lands on the
                                    // new colour.
                                    onClicked: {
                                        pane.controller.setPreference(
                                            "appearance",
                                            "wallpaper_" + pane.scheme + "_color", modelData);
                                        pane.controller.setPreference(
                                            "appearance",
                                            "wallpaper_" + pane.scheme + "_source", "color");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // A solid colour has nothing to frame, and its image is generated at the
        // largest monitor's size, so the helper pins it to cover.
        SettingsGroup {
            title: "FILL"
            visible: !pane.isColor("light") || !pane.isColor("dark")

            SettingsRow {
                title: "Fill"
                description: "How a picture fits each display. Shared by both appearances."
                SettingsCombo {
                    model: ["Cover", "Contain", "Fit", "Tile"]
                    currentIndex: Math.max(0, ["cover", "contain", "fit", "tile"].indexOf(
                        pane.controller.preference("appearance", "wallpaper_fit", "cover")))
                    onActivated: index => pane.controller.setPreference(
                        "appearance", "wallpaper_fit", ["cover", "contain", "fit", "tile"][index])
                }
            }
        }

        Item { Layout.preferredHeight: 20 }
    }
}
