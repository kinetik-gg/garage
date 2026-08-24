import Quickshell
import QtQuick

// One extension-owned popover. Content can be a Component or a QML URL and is
// injected with the same bar/services/manifest/probe contract as Widget.qml.
Scope {
    id: popover

    required property var bar
    required property var services
    required property var manifest
    property var probe: null
    property var content: null
    property var properties: ({})
    property bool open: false

    function show(nextContent, nextProperties) {
        if (open) {
            close();
            return;
        }
        content = nextContent;
        properties = nextProperties || ({});
        open = true;
    }

    function close() {
        if (surfaceLoader.item)
            surfaceLoader.item.dismissSurface();
        else
            open = false;
    }

    DismissCatcher {
        active: popover.open
        onDismissed: popover.close()
    }

    LazyLoader {
        id: surfaceLoader
        active: popover.open

        PaletteSurface {
            id: popupSurface
            targetScreen: popover.bar.screen
            edge: popover.bar.edge
            targetAnchor: popover.bar.anchorPosition()
            implicitWidth: Math.max(240, contentHost.hostedItem
                ? contentHost.hostedItem.implicitWidth : 0)
            implicitHeight: Math.max(120, contentHost.hostedItem
                ? contentHost.hostedItem.implicitHeight : 0)
            onDismissed: popover.open = false

            Rectangle {
                anchors.fill: parent
                color: Theme.contentTint
                opacity: popupSurface.contentOpacity

                Item {
                    id: contentHost
                    anchors.fill: parent
                    property var createdItem: null
                    readonly property var hostedItem: contentLoader.item || createdItem

                    function loadContent() {
                        const injected = Object.assign({}, popover.properties, {
                            bar: popover.bar,
                            services: popover.services,
                            manifest: popover.manifest,
                            probe: popover.probe
                        });
                        if (popover.content && popover.content.createObject) {
                            createdItem = popover.content.createObject(contentHost, injected);
                        } else if (popover.content) {
                            contentLoader.setSource(String(popover.content), injected);
                        }
                    }

                    Loader {
                        id: contentLoader
                        anchors.fill: parent
                    }

                    Component.onCompleted: loadContent()
                }
            }
        }
    }
}
