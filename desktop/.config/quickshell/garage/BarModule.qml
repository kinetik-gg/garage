import QtQuick
import Qt5Compat.GraphicalEffects

// Loads one registered Widget.qml and is the injected `bar` API visible to it.
Item {
    id: module

    required property string extensionId
    required property var registry
    required property var services
    required property var screen
    required property string screenName
    property string edge: BarState.position
    property Item anchorItem: module

    readonly property bool vertical: edge === "left" || edge === "right"
    readonly property int thickness: BarState.thickness
    readonly property var theme: Theme
    readonly property var spacing: ({
        edge: BarState.scaled("edge"),
        menuRight: BarState.scaled("menuRight"),
        workspaceGap: BarState.scaled("workspaceGap"),
        module: BarState.scaled("module"),
        image: BarState.scaled("image"),
        icon: BarState.scaled("icon"),
        tray: BarState.scaled("tray"),
        tooltip: BarState.scaled("tooltip")
    })
    readonly property var extension: registry.lookup(extensionId)
    readonly property var manifest: extension ? extension.manifest : ({})
    readonly property var probe: ProbeHost.lookup(extensionId)
    readonly property bool structurallyVisible: extension !== null
        && (!vertical || extension.widget.vertical !== "hide")
    // Keep the host alive while an asynchronous widget has no content. Folding the
    // child's effective visibility back into this parent's visibility creates a
    // deadlock: Qt propagates a hidden parent to its children, so a child cannot
    // become effectively visible when its service later publishes data.
    readonly property bool contentVisible: !widgetLoader.item
        || widgetLoader.item.visible

    signal surfaceRequested(string surface, string screenName, real anchor)

    visible: structurallyVisible
    implicitWidth: structurallyVisible && contentVisible ? (vertical ? thickness
        : Math.max(thickness, widgetLoader.item
            ? widgetLoader.item.implicitWidth : thickness)) : 0
    implicitHeight: structurallyVisible && contentVisible ? (vertical ? Math.max(thickness,
        widgetLoader.item ? widgetLoader.item.implicitHeight : thickness)
        : thickness) : 0
    width: implicitWidth
    height: implicitHeight

    function scaled(name) { return BarState.scaled(name); }

    function anchorPosition(item) {
        const target = item || anchorItem || module;
        const point = target.mapToItem(null, target.width / 2, target.height / 2);
        return vertical ? point.y : point.x;
    }

    function openSurface(name, item) {
        surfaceRequested(String(name), screenName, anchorPosition(item));
    }

    function openPopup(content, properties) {
        popover.show(content, properties);
    }

    function closePopup() { popover.close(); }

    function loadWidget() {
        widgetLoader.source = "";
        widgetLoader.sourceComponent = null;
        if (!extension)
            return;
        if (extension.widget.inline === true) {
            widgetLoader.setSource(extension.widgetUrl, {
                bar: module,
                services: module.services,
                manifest: extension.manifest,
                probe: module.probe
            });
        } else {
            widgetLoader.sourceComponent = fallbackComponent;
        }
    }

    Loader {
        id: widgetLoader
        anchors.centerIn: parent
    }

    Component {
        id: fallbackComponent

        Item {
            id: fallback
            implicitWidth: 24 + module.spacing.icon
            implicitHeight: 24

            readonly property string iconName: module.extension
                ? module.extension.widget.icon : ""
            readonly property string iconPath: iconName.indexOf("/") >= 0
                ? module.extension.root + "/" + iconName
                : GaragePaths.shellDir + "/icons/" + iconName + ".svg"

            Rectangle {
                anchors.fill: parent
                radius: 8
                color: pointer.pressed ? Qt.alpha(Theme.text, 0.22)
                    : pointer.containsMouse ? Qt.alpha(Theme.text, 0.12)
                    : "transparent"
            }

            Image {
                id: fallbackIcon
                anchors.centerIn: parent
                width: 16
                height: 16
                source: fallback.iconPath
                sourceSize.width: 32
                sourceSize.height: 32
                fillMode: Image.PreserveAspectFit
                smooth: true
                visible: false
            }

            ColorOverlay {
                anchors.fill: fallbackIcon
                source: fallbackIcon
                color: Theme.text
                cached: true
            }

            MouseArea {
                id: pointer
                anchors.fill: parent
                hoverEnabled: true
                enabled: module.extension && (module.extension.popupUrl !== ""
                    || typeof module.extension.widget.surface === "string")
                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: {
                    if (module.extension.popupUrl !== "")
                        module.openPopup(module.extension.popupUrl, {});
                    else if (typeof module.extension.widget.surface === "string")
                        module.openSurface(module.extension.widget.surface, fallback);
                }
            }
        }
    }

    WidgetPopover {
        id: popover
        bar: module
        services: module.services
        manifest: module.manifest
        probe: module.probe
    }

    onExtensionChanged: loadWidget()
    onProbeChanged: loadWidget()
    Component.onCompleted: loadWidget()
}
