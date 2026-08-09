import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts
import Qt.labs.folderlistmodel
import Qt5Compat.GraphicalEffects

Scope {
    id: picker
    required property var controller
    required property string targetScreenName
    // Which preference the chosen file is written to. Required rather than
    // defaulted: the wallpaper is per appearance, so a wrong-but-plausible
    // default would silently dress the half the user is not looking at.
    required property string targetKey
    // Where the browser opens. Empty leaves it wherever it was last left, which
    // is what reopening the picker during one visit should do.
    property string startFolder: ""
    // The window this picker belongs to, so it follows it across monitors.
    property var parentWindow: null
    property bool pickerVisible: false
    property url selectedUrl: ""
    property bool selectedIsDir: false
    signal dismissed()

    function targetScreen() {
        for (let index = 0; index < Quickshell.screens.length; ++index)
            if (Quickshell.screens[index].name === targetScreenName) return Quickshell.screens[index];
        return Quickshell.screens.length ? Quickshell.screens[0] : null;
    }
    function localPath(url) {
        return decodeURIComponent(url.toString().replace(/^file:\/\//, ""));
    }
    function chooseSelection() {
        if (!selectedUrl.toString()) return;
        if (selectedIsDir) {
            files.folder = selectedUrl;
            selectedUrl = "";
            return;
        }
        controller.setPreference("appearance", picker.targetKey, localPath(selectedUrl));
        dismissed();
    }

    // Reopening on a stale selection would arm Choose with a file the list is no
    // longer showing, so the browser is reset to its start folder each time.
    onPickerVisibleChanged: {
        if (!pickerVisible)
            return;
        selectedUrl = "";
        selectedIsDir = false;
        if (picker.startFolder !== "")
            files.folder = "file://" + encodeURI(picker.startFolder);
    }

    FolderListModel {
        id: files
        folder: "file://" + Quickshell.env("HOME") + "/Pictures"
        rootFolder: "file://" + Quickshell.env("HOME")
        nameFilters: ["*.png", "*.jpg", "*.jpeg", "*.webp", "*.jxl", "*.bmp"]
        showDirs: true
        showDotAndDotDot: false
        sortField: FolderListModel.Name
    }

    // A transient child of the window that opened it. Pinning it to a screen
    // name captured when preferences first opened left it behind on the original
    // monitor once that window could be dragged; parenting makes the compositor
    // place it over whatever monitor the parent is on now.
    FloatingWindow {
        id: window
        visible: picker.pickerVisible
        parentWindow: picker.parentWindow
        title: "Choose Desktop Picture"
        implicitWidth: 280
        implicitHeight: 480
        minimumSize: Qt.size(240, 320)
        color: "transparent"
        surfaceFormat.opaque: false

        Rectangle {
            anchors.fill: parent
            color: Theme.dialogTint

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 8

                RowLayout {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 32
                    Text {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        text: "Choose Desktop Picture"
                        color: Theme.text
                        font.family: Theme.sans
                        font.pixelSize: 16
                        font.weight: Font.Bold
                        elide: Text.ElideRight
                        renderType: Text.NativeRendering
                    }
                    ContinuousRectangle {
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 32
                        radius: Theme.controlRadius
                        color: closePointer.containsMouse ? Theme.hoverStrong : "transparent"
                        Image {
                            id: closeGlyph
                            anchors.centerIn: parent
                            width: 16; height: 16
                            source: "icons/x.svg"
                            sourceSize.width: 36; sourceSize.height: 36
                            smooth: true; antialiasing: true; mipmap: true
                            visible: false
                        }

                        // The svg carries its own colour, so without an overlay
                        // it stays fixed while the theme moves around it.
                        ColorOverlay {
                            anchors.fill: closeGlyph
                            source: closeGlyph
                            color: Theme.text
                            cached: true
                        }
                        MouseArea {
                            id: closePointer
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: picker.dismissed()
                        }
                    }
                }

                ContinuousRectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: Theme.hover
                    borderWidth: 1
                    borderColor: Theme.frameInner

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 6
                        spacing: 4

                        RowLayout {
                            Layout.fillWidth: true
                            SettingsButton {
                                text: "Up"
                                iconSource: "icons/arrow-up.svg"
                                horizontalPadding: 10
                                verticalPadding: 6
                                enabled: files.folder.toString() !== files.rootFolder.toString()
                                onClicked: { files.folder = files.parentFolder; picker.selectedUrl = ""; }
                            }
                            Text {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                text: picker.localPath(files.folder)
                                color: Theme.textMuted
                                font.family: Theme.sans
                                font.pixelSize: 11
                                elide: Text.ElideMiddle
                                renderType: Text.NativeRendering
                            }
                        }
                        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.frameInner }

                        ListView {
                            id: fileList
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            model: files
                            spacing: 2
                            boundsBehavior: Flickable.StopAtBounds
                            delegate: ContinuousRectangle {
                                id: row
                                required property string fileName
                                required property url fileUrl
                                required property bool fileIsDir
                                readonly property bool highlighted:
                                    picker.selectedUrl.toString() === row.fileUrl.toString()
                                    || rowPointer.containsMouse
                                width: fileList.width
                                height: 40
                                radius: Theme.controlRadius
                                color: highlighted ? Theme.accent : "transparent"

                                Image {
                                    id: previewSource
                                    anchors.left: parent.left
                                    anchors.leftMargin: 5
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 44; height: 29
                                    source: row.fileIsDir ? "" : row.fileUrl
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                    sourceSize.width: 116; sourceSize.height: 76
                                    smooth: true
                                    visible: false
                                }
                                ContinuousRectangle {
                                    id: previewMask
                                    anchors.fill: previewSource
                                    color: "white"
                                    radius: Theme.controlRadius
                                    visible: false
                                }
                                OpacityMask {
                                    anchors.fill: previewSource
                                    source: previewSource
                                    maskSource: previewMask
                                    cached: true
                                    visible: !row.fileIsDir
                                }
                                ContinuousRectangle {
                                    anchors.fill: previewSource
                                    visible: row.fileIsDir
                                    color: Theme.surface
                                    radius: Theme.controlRadius
                                    Image {
                                        id: folderGlyph
                                        anchors.centerIn: parent
                                        width: 20
                                        height: 20
                                        source: "icons/folder.svg"
                                        sourceSize.width: 40
                                        sourceSize.height: 40
                                        smooth: true
                                        antialiasing: true
                                        mipmap: true
                                        visible: false
                                    }

                                    ColorOverlay {
                                        anchors.fill: folderGlyph
                                        source: folderGlyph
                                        color: Theme.textMuted
                                        cached: true
                                    }
                                }
                                Text {
                                    anchors.left: previewSource.right
                                    anchors.leftMargin: 9
                                    anchors.right: parent.right
                                    anchors.rightMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: row.fileName
                                    color: row.highlighted ? Theme.accentText : Theme.text
                                    font.family: Theme.sans
                                    font.pixelSize: 12
                                    elide: Text.ElideMiddle
                                    renderType: Text.NativeRendering
                                }
                                MouseArea {
                                    id: rowPointer
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        picker.selectedUrl = row.fileUrl;
                                        picker.selectedIsDir = row.fileIsDir;
                                    }
                                    onDoubleClicked: {
                                        picker.selectedUrl = row.fileUrl;
                                        picker.selectedIsDir = row.fileIsDir;
                                        picker.chooseSelection();
                                    }
                                }
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Item { Layout.fillWidth: true }
                    SettingsButton {
                        text: "Cancel"
                        horizontalPadding: 10
                        verticalPadding: 6
                        onClicked: picker.dismissed()
                    }
                    SettingsButton {
                        text: picker.selectedIsDir ? "Open" : "Choose"
                        prominent: true
                        horizontalPadding: 10
                        verticalPadding: 6
                        enabled: picker.selectedUrl.toString() !== ""
                        onClicked: picker.chooseSelection()
                    }
                }
            }
        }
        Shortcut { sequence: "Escape"; onActivated: picker.dismissed() }
    }

}
