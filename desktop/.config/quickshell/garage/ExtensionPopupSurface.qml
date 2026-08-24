import QtQuick

// Hosts extension-owned popup content when a global IPC route has no concrete
// BarModule to own WidgetPopover. The extension still owns every pixel inside
// the surface; this host only supplies screen/edge placement and the standard
// bar services contract.
PaletteSurface {
    id: surface

    required property string extensionId

    readonly property var entry: ExtensionRegistry.lookup(extensionId)
    readonly property var extensionServices: ProbeHost.services
    readonly property var barContract: ({
        theme: Theme,
        screen: surface.effectiveScreen,
        edge: surface.edge,
        spacing: ({
            edge: BarState.scaled("edge"),
            menuRight: BarState.scaled("menuRight"),
            workspaceGap: BarState.scaled("workspaceGap"),
            module: BarState.scaled("module"),
            image: BarState.scaled("image"),
            icon: BarState.scaled("icon"),
            tray: BarState.scaled("tray"),
            tooltip: BarState.scaled("tooltip")
        })
    })

    edge: BarState.position
    surfaceNamespace: "garage-extension-" + extensionId
    implicitWidth: Math.max(240, contentLoader.item
        ? contentLoader.item.implicitWidth : 0)
    implicitHeight: Math.max(120, contentLoader.item
        ? contentLoader.item.implicitHeight : 0)

    function requestDismissal() {
        dismissSurface();
    }

    function loadContent() {
        if (!entry || entry.popupUrl === "") {
            contentLoader.source = "";
            return;
        }
        contentLoader.setSource(entry.popupUrl, {
            bar: surface.barContract,
            services: surface.extensionServices,
            manifest: entry.manifest,
            probe: ProbeHost.lookup(surface.extensionId)
        });
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.contentTint
        opacity: surface.contentOpacity

        Loader {
            id: contentLoader
            anchors.fill: parent
        }
    }

    onEntryChanged: loadContent()
    Component.onCompleted: loadContent()
}
