import Quickshell
import QtQuick
import Qt5Compat.GraphicalEffects
import QtQuick.Layouts

// One notification, drawn once. The toast stack shows these and the notification
// centre will show the same component in its list, so the two cannot drift into
// two different pictures of the same notification.
//
// The card never closes a notification by itself: it says what was asked of it and
// animates itself out when its owner says to. A toast ending and a notification
// being dismissed are different things -- one keeps the notification in history
// and the other does not -- and only the owner knows which one this is.
ContinuousRectangle {
    id: card

    required property var notification
    // Toast mode: the body is clamped, because a popup is a glance and a wall of
    // text pushes the stack off the screen.
    property bool compact: false
    // Set by the owner to play the card out. exited() follows when the animation
    // has finished and the notification is safe to close.
    property bool exiting: false

    signal closeRequested()
    signal exited()

    readonly property bool live: notification !== null && notification !== undefined
    readonly property string imageSource: live ? String(notification.image || "") : ""
    readonly property var actions: live ? notification.actions : []

    // appIcon already falls back to the icon of the sender's desktop entry, so
    // this only has the icon theme and the built-in glyph left to try.
    readonly property string appIcon: {
        if (!card.live)
            return "";
        const icon = String(card.notification.appIcon || "");
        if (icon === "")
            return "";
        // A sender may hand over a path instead of a theme name, and iconPath
        // would look a path up as a name and find nothing.
        if (icon.startsWith("/") || icon.startsWith("file:") || icon.startsWith("image:"))
            return icon;
        return Quickshell.iconPath(icon, true);
    }
    readonly property bool usesGlyph: appIcon === ""

    function relativeTime(reference) {
        if (!card.live)
            return "";
        const elapsed = reference.getTime() - NotificationDaemon.arrivalTime(card.notification);
        if (elapsed < 60000)
            return "now";
        const minutes = Math.floor(elapsed / 60000);
        if (minutes < 60)
            return minutes + "m";
        const hours = Math.floor(minutes / 60);
        if (hours < 24)
            return hours + "h";
        return Math.floor(hours / 24) + "d";
    }

    implicitWidth: 390
    implicitHeight: body.implicitHeight + 24
    radius: Theme.cornerRadius
    // A toast floats over live desktop content with no glass drawn beneath it --
    // garage-notifications is not one of the compositor's glass layer namespaces --
    // so the card carries its own body, the same one the glass-backed panels use
    // over their material.
    color: Theme.contentTint
    borderWidth: 1
    borderColor: Theme.frameOuter

    // Layout-neutral so the slide cannot disturb the stack it is leaving: a
    // transform moves the painted card without moving the item the column sized.
    transform: Translate { id: exitShift }

    // Held only while the card is playing out. A notification the sender closes
    // mid-animation would otherwise be destroyed under the bindings below; a lock
    // held for the card's whole life instead would keep closed notifications in
    // the tracked model and the owner would never see them leave.
    RetainableLock {
        object: card.notification
        locked: card.exiting
    }

    // Appear rather than materialise. Starts itself on completion, which is
    // exactly when a card is created for a notification that just arrived.
    NumberAnimation on opacity {
        from: 0
        to: 1
        duration: 130
        easing.type: Easing.OutCubic
    }

    SequentialAnimation {
        id: exitAnimation

        ParallelAnimation {
            NumberAnimation {
                target: card
                property: "opacity"
                to: 0
                duration: 150
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: exitShift
                property: "x"
                to: 28
                duration: 150
                easing.type: Easing.OutCubic
            }
        }

        onFinished: card.exited()
    }

    // Driven from the handler rather than by binding exitAnimation.running: an
    // animation assigns its own running property when it finishes, which would
    // break a binding there and leave the second exit silent -- and a silent exit
    // never emits exited(), which is what a toast waits on to end.
    onExitingChanged: {
        if (card.exiting) {
            exitAnimation.restart();
            return;
        }
        exitAnimation.stop();
        card.opacity = 1;
        exitShift.x = 0;
    }

    // Inner hairline, one inset px in from the outer one, the same double frame
    // the menus and the settings groups draw. insetRadius keeps the two concentric
    // at every corner radius setting.
    ContinuousRectangle {
        anchors.fill: parent
        anchors.margins: 1
        radius: Theme.insetRadius(card.radius, 1)
        borderWidth: 1
        borderColor: Theme.frameInner
    }

    ColumnLayout {
        id: body
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Item {
                Layout.preferredWidth: 18
                Layout.preferredHeight: 18
                Layout.alignment: Qt.AlignVCenter

                // Theme icons arrive already coloured and must not be overlaid.
                Image {
                    anchors.fill: parent
                    visible: !card.usesGlyph
                    source: card.usesGlyph ? "" : card.appIcon
                    sourceSize.width: 36
                    sourceSize.height: 36
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    mipmap: true
                }

                // The built-in glyph ships its own colour, so it is drawn through
                // an overlay to follow the theme rather than staying fixed.
                Image {
                    id: bellGlyph
                    anchors.fill: parent
                    visible: false
                    source: card.usesGlyph ? "icons/bell.svg" : ""
                    sourceSize.width: 36
                    sourceSize.height: 36
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    antialiasing: true
                    mipmap: true
                }

                ColorOverlay {
                    anchors.fill: bellGlyph
                    source: bellGlyph
                    visible: card.usesGlyph
                    color: Theme.textMuted
                    cached: true
                }
            }

            Text {
                Layout.fillWidth: true
                text: card.live ? String(card.notification.appName || "Notification") : ""
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }

            Text {
                text: card.relativeTime(NotificationDaemon.now)
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                renderType: Text.NativeRendering
            }

            ContinuousRectangle {
                implicitWidth: 20
                implicitHeight: 20
                radius: Theme.controlRadius
                color: closePointer.containsMouse ? Theme.hoverStrong : "transparent"

                Image {
                    id: closeGlyph
                    anchors.centerIn: parent
                    width: 12
                    height: 12
                    source: "icons/x.svg"
                    sourceSize.width: 24
                    sourceSize.height: 24
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    antialiasing: true
                    mipmap: true
                    visible: false
                }

                ColorOverlay {
                    anchors.fill: closeGlyph
                    source: closeGlyph
                    color: closePointer.containsMouse ? Theme.text : Theme.textMuted
                    cached: true
                }

                MouseArea {
                    id: closePointer
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: card.closeRequested()
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            // The sender's own picture -- a profile photo, an album cover. Drawn
            // to fit rather than cropped to a rounded tile: fitting needs no mask,
            // and a mask over a superellipse is a second silhouette to keep in
            // step with the corner radius setting for no gain at 42 px.
            Image {
                Layout.preferredWidth: 42
                Layout.preferredHeight: 42
                Layout.alignment: Qt.AlignTop
                visible: card.imageSource !== ""
                source: card.imageSource
                sourceSize.width: 84
                sourceSize.height: 84
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                Text {
                    id: summaryText
                    Layout.fillWidth: true
                    text: card.live ? String(card.notification.summary || "") : ""
                    visible: summaryText.text !== ""
                    color: Theme.text
                    font.family: Theme.sans
                    font.pixelSize: 13
                    font.weight: Font.Bold
                    wrapMode: Text.WordWrap
                    maximumLineCount: card.compact ? 2 : 4
                    elide: Text.ElideRight
                    renderType: Text.NativeRendering
                }

                Text {
                    id: bodyText
                    Layout.fillWidth: true
                    text: card.live ? String(card.notification.body || "") : ""
                    visible: bodyText.text !== ""
                    color: Theme.textMuted
                    font.family: Theme.sans
                    font.pixelSize: 12
                    lineHeight: 1.15
                    wrapMode: Text.WordWrap
                    maximumLineCount: card.compact ? 4 : 12
                    elide: Text.ElideRight
                    // The server advertises body markup, so senders send the
                    // freedesktop subset -- b, i, u, a, img -- and StyledText is
                    // the format that reads exactly that and ignores the rest.
                    // PlainText here would print the tags; RichText would hand a
                    // sender a full HTML document to lay the shell out with.
                    textFormat: Text.StyledText
                    renderType: Text.NativeRendering
                }
            }
        }

        Flow {
            Layout.fillWidth: true
            visible: card.actions.length > 0
            spacing: 6

            Repeater {
                model: card.actions

                SettingsButton {
                    required property var modelData
                    text: String(modelData.text || "")
                    // Ghost: the notification is the content, and a row of filled
                    // buttons on a card this small reads as the card.
                    ghost: true
                    verticalPadding: 5
                    horizontalPadding: 10
                    // Invoked straight away, with no exit animation: invoking
                    // destroys the notification unless it is resident, so there
                    // would be nothing left to animate. The card's owner drops the
                    // toast when the notification goes.
                    onClicked: modelData.invoke()
                }
            }
        }
    }
}
