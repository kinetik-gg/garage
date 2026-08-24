import QtQuick
import Qt5Compat.GraphicalEffects

// One ordered composition rail. maxWidgets counts widgets only; the chevron is
// an extra control. Expansion stays inline on this rail and this output.
Item {
    id: rail

    required property var registry
    required property var services
    required property var screen
    required property string screenName
    required property string railRole
    property var extensionIds: []
    property string edge: BarState.position
    property int maxWidgets: BarState.maxGroupWidgets
    property bool expanded: false

    readonly property bool vertical: edge === "left" || edge === "right"
    readonly property var validIds: filteredIds()
    readonly property bool overflowing: validIds.length > maxWidgets
    readonly property var displayedIds: visibleIds()

    signal surfaceRequested(string surface, string screenName, real anchor)

    implicitWidth: vertical ? verticalContent.implicitWidth
        : horizontalContent.implicitWidth
    implicitHeight: vertical ? verticalContent.implicitHeight
        : horizontalContent.implicitHeight
    width: implicitWidth
    height: implicitHeight

    function filteredIds() {
        const ignored = registry.revision;
        const result = [];
        for (const id of extensionIds) {
            const entry = registry.lookup(id);
            if (entry && (!vertical || entry.widget.vertical !== "hide"))
                result.push(id);
        }
        return result;
    }

    function visibleIds() {
        if (expanded || !overflowing)
            return validIds;
        if (railRole === "right")
            return validIds.slice(validIds.length - maxWidgets);
        return validIds.slice(0, maxWidgets);
    }

    function caretDirection() {
        if (vertical) {
            const towardHidden = railRole === "right" ? "up" : "down";
            return expanded ? (towardHidden === "up" ? "down" : "up")
                : towardHidden;
        }
        const towardHidden = railRole === "right" ? "left" : "right";
        return expanded ? (towardHidden === "left" ? "right" : "left")
            : towardHidden;
    }

    component ChevronButton: Item {
        visible: rail.overflowing
        implicitWidth: visible ? (rail.vertical ? BarState.thickness : 24) : 0
        implicitHeight: visible ? (rail.vertical ? 24 : BarState.thickness) : 0
        width: implicitWidth
        height: implicitHeight

        Rectangle {
            anchors.fill: parent
            radius: 8
            color: pointer.pressed ? Qt.alpha(Theme.text, 0.22)
                : pointer.containsMouse ? Qt.alpha(Theme.text, 0.12)
                : "transparent"
        }

        Image {
            id: caret
            anchors.centerIn: parent
            width: 14
            height: 14
            source: GaragePaths.shellDir + "/icons/caret-"
                + rail.caretDirection() + ".svg"
            sourceSize.width: 28
            sourceSize.height: 28
            fillMode: Image.PreserveAspectFit
            smooth: true
            visible: false
        }

        ColorOverlay {
            anchors.fill: caret
            source: caret
            color: Theme.text
            cached: true
        }

        MouseArea {
            id: pointer
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: rail.expanded = !rail.expanded
        }
    }

    component ExtensionModule: BarModule {
        required property string modelData
        extensionId: modelData
        registry: rail.registry
        services: rail.services
        screen: rail.screen
        screenName: rail.screenName
        edge: rail.edge
        onSurfaceRequested: (surface, name, anchor) =>
            rail.surfaceRequested(surface, name, anchor)
    }

    Row {
        id: horizontalContent
        visible: !rail.vertical
        spacing: BarState.scaled("module")

        ChevronButton { visible: rail.overflowing && rail.railRole === "right" }

        Repeater {
            model: rail.vertical ? [] : rail.displayedIds
            delegate: ExtensionModule {}
        }

        ChevronButton { visible: rail.overflowing && rail.railRole !== "right" }
    }

    Column {
        id: verticalContent
        visible: rail.vertical
        spacing: BarState.scaled("module")

        ChevronButton { visible: rail.overflowing && rail.railRole === "right" }

        Repeater {
            model: rail.vertical ? rail.displayedIds : []
            delegate: ExtensionModule {}
        }

        ChevronButton { visible: rail.overflowing && rail.railRole !== "right" }
    }

    onExtensionIdsChanged: expanded = false
    onMaxWidgetsChanged: expanded = false
}
