pragma ComponentBehavior: Bound

import Quickshell
import QtQuick
import QtQuick.Layouts
import Qt.labs.folderlistmodel
import Qt5Compat.GraphicalEffects

Scope {
    id: picker

    required property var parentWindow
    property bool pickerVisible: false
    signal chosen(string path)
    signal dismissed()

    function localPath(url) {
        return decodeURIComponent(String(url).replace(/^file:\/\//, ""));
    }

    function chooseCurrent() {
        picker.chosen(picker.localPath(directories.folder));
        picker.dismissed();
    }

    onPickerVisibleChanged: if (pickerVisible)
        directories.folder = "file://" + encodeURI(Quickshell.env("HOME"));

    FolderListModel {
        id: directories
        folder: "file://" + Quickshell.env("HOME")
        rootFolder: "file://" + Quickshell.env("HOME")
        showDirs: true
        showFiles: false
        showDotAndDotDot: false
        sortField: FolderListModel.Name
    }

    FloatingWindow {
        id: window
        visible: picker.pickerVisible
        parentWindow: picker.parentWindow
        title: "Add Indexed Folder"
        implicitWidth: 360
        implicitHeight: 520
        minimumSize: Qt.size(300, 360)
        color: "transparent"
        surfaceFormat.opaque: false

        Rectangle {
            anchors.fill: parent
            color: Theme.contentTint

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 14
                spacing: 10

                RowLayout {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 34

                    Text {
                        Layout.fillWidth: true
                        text: "Add Indexed Folder"
                        color: Theme.text
                        font.family: Theme.sans
                        font.pixelSize: 17
                        font.weight: Font.Bold
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
                            width: 16
                            height: 16
                            source: "icons/x.svg"
                            sourceSize.width: 32
                            sourceSize.height: 32
                            visible: false
                        }
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

                Text {
                    Layout.fillWidth: true
                    text: "Choose a folder inside your home directory. Hidden and generated dependency folders below it are skipped."
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    renderType: Text.NativeRendering
                }

                ContinuousRectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: Theme.hover
                    borderWidth: 1
                    borderColor: Theme.frameInner

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 7
                        spacing: 5

                        RowLayout {
                            Layout.fillWidth: true
                            SettingsButton {
                                text: "Up"
                                iconSource: "icons/arrow-up.svg"
                                horizontalPadding: 10
                                verticalPadding: 6
                                enabled: String(directories.folder)
                                    !== String(directories.rootFolder)
                                onClicked: directories.folder = directories.parentFolder
                            }
                            Text {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                text: picker.localPath(directories.folder)
                                color: Theme.textMuted
                                font.family: Theme.mono
                                font.pixelSize: 10
                                elide: Text.ElideMiddle
                                renderType: Text.NativeRendering
                            }
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 1
                            color: Theme.frameInner
                        }

                        ListView {
                            id: folderList
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            model: directories
                            spacing: 2
                            boundsBehavior: Flickable.StopAtBounds

                            delegate: ContinuousRectangle {
                                id: row
                                required property string fileName
                                required property url fileUrl
                                width: folderList.width
                                height: 38
                                radius: Theme.controlRadius
                                color: rowPointer.containsMouse ? Theme.hoverStrong : "transparent"

                                Image {
                                    id: folderGlyph
                                    anchors.left: parent.left
                                    anchors.leftMargin: 8
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 20
                                    height: 20
                                    source: "icons/folder.svg"
                                    sourceSize.width: 40
                                    sourceSize.height: 40
                                    visible: false
                                }
                                ColorOverlay {
                                    anchors.fill: folderGlyph
                                    source: folderGlyph
                                    color: Theme.textMuted
                                    cached: true
                                }
                                Text {
                                    anchors.left: folderGlyph.right
                                    anchors.leftMargin: 10
                                    anchors.right: parent.right
                                    anchors.rightMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: row.fileName
                                    color: Theme.text
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
                                    onClicked: directories.folder = row.fileUrl
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
                        onClicked: picker.dismissed()
                    }
                    SettingsButton {
                        text: "Add This Folder"
                        prominent: true
                        onClicked: picker.chooseCurrent()
                    }
                }
            }
        }

        Shortcut { sequence: "Escape"; onActivated: picker.dismissed() }
    }
}
