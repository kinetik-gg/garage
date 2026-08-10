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
    // text pushes the stack off the screen. It also drops the affordances that
    // only make sense in a list that stays put -- the reply field and the clock
    // time -- and draws its identity as a meta row above the title rather than
    // beside an icon, which is the shape the toast was approved in.
    property bool compact: false
    // The top of a closed stack in the centre. The card is still the whole card,
    // but it is standing in for the ones behind it: the body is cut to a glance
    // and the affordances that need a row which is going to stay put -- the
    // actions and the reply field -- wait until the stack is opened.
    property bool collapsed: false
    // What is left underneath, e.g. "2 more". Drawn in the card's own top line
    // because the stack is the group: there is no header above it to hang a
    // count on any more.
    property string stackNote: ""
    // Set by the owner to play the card out. exited() follows when the animation
    // has finished and the notification is safe to close.
    property bool exiting: false

    signal closeRequested()
    signal exited()
    // A reply went out. The card does not close the notification for the same
    // reason it never closes one anywhere else: only the owner knows whether the
    // row should play out or stay.
    signal replied()

    readonly property bool live: notification !== null && notification !== undefined
    readonly property string imageSource: live ? String(notification.image || "") : ""
    readonly property var actions: live ? notification.actions : []

    // Shared between the toast's meta row and the centre card's title row, so
    // the two spellings of "when, and how to close" cannot drift apart.
    component TimeLabel: Text {
        text: card.timeLabel(NotificationDaemon.now)
        color: Theme.textMuted
        font.family: Theme.sans
        font.pixelSize: 11
        renderType: Text.NativeRendering
    }

    // The sender's icon, drawn the same way wherever it appears: the resolved
    // theme icon when there is one, the shell's own bell when there is not.
    // Shared between the toast's meta row and the centre card's leading tile so
    // a sender cannot be one picture in a popup and another in the list.
    component AppIcon: Item {
        id: iconTile

        // Rasterisation size, passed rather than derived from the width: the
        // toast's icon is approved as it is drawn today, and a size the layout
        // computes would re-raster it into something slightly else.
        property int pixels: 36

        // Theme icons arrive already coloured and must not be overlaid.
        Image {
            anchors.fill: parent
            visible: !card.usesGlyph
            source: card.usesGlyph ? "" : card.appIcon
            sourceSize.width: iconTile.pixels
            sourceSize.height: iconTile.pixels
            fillMode: Image.PreserveAspectFit
            smooth: true
            mipmap: true
        }

        // The built-in glyph ships its own colour, so it is drawn through an
        // overlay to follow the theme rather than staying fixed.
        Image {
            id: bellGlyph
            anchors.fill: parent
            visible: false
            source: card.usesGlyph ? "icons/bell.svg" : ""
            sourceSize.width: iconTile.pixels
            sourceSize.height: iconTile.pixels
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

    component CloseButton: ContinuousRectangle {
        implicitWidth: 20
        implicitHeight: 20
        radius: Theme.controlRadius
        color: closeArea.containsMouse ? Theme.hoverStrong : "transparent"

        Image {
            id: closeGlyphImage
            anchors.centerIn: parent
            width: 12
            height: 12
            source: "icons/x.svg"
            sourceSize.width: 24
            sourceSize.height: 24
            fillMode: Image.PreserveAspectFit
            smooth: true
            mipmap: true
            visible: false
        }

        ColorOverlay {
            anchors.fill: closeGlyphImage
            source: closeGlyphImage
            color: closeArea.containsMouse ? Theme.text : Theme.textMuted
            cached: true
        }

        MouseArea {
            id: closeArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: card.closeRequested()
        }
    }

    // Inline reply. Only off a toast: a popup times out under the pointer, and a
    // text field that can disappear mid-sentence is a trap. This field is the
    // whole reason NotificationDaemon advertises the capability at all.
    // Nor off the top of a closed stack: the field is there to be typed into,
    // and a click anywhere in a closed stack opens it instead.
    readonly property bool canReply: !card.compact && !card.collapsed && card.live
        && card.notification.hasInlineReply
    // Whether the field has the keyboard. The centre watches this so it can stop
    // rebuilding its list while a reply is being typed into one of its rows.
    readonly property bool replyActive: replyField.activeFocus

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

    // What the card prints for its age, given the shell's minute clock. A toast
    // gets the short relative form -- it is on screen for seconds, and the only
    // question it answers is whether this is the notification that just arrived.
    // The centre gets the clock time once a notification is old enough for "3d"
    // to have stopped being an answer.
    function timeLabel(reference) {
        if (!card.live)
            return "";
        const arrival = NotificationDaemon.arrivalTime(card.notification);
        const elapsed = reference.getTime() - arrival;
        if (elapsed < 60000)
            return "now";
        const minutes = Math.floor(elapsed / 60000);

        if (card.compact) {
            if (minutes < 60)
                return minutes + "m";
            const hours = Math.floor(minutes / 60);
            if (hours < 24)
                return hours + "h";
            return Math.floor(hours / 24) + "d";
        }

        if (minutes < 60)
            return minutes + "m ago";

        const when = new Date(arrival);
        if (when.toDateString() === reference.toDateString())
            return Qt.formatTime(when, Locale.ShortFormat);
        const yesterday = new Date(reference.getTime() - 86400000);
        if (when.toDateString() === yesterday.toDateString())
            return "Yesterday";
        // Formatted through the locale rather than a literal pattern: the shell
        // has no business deciding whether the user writes the day or the month
        // first.
        return Qt.formatDate(when, Locale.ShortFormat);
    }

    function sendReply() {
        if (!card.canReply)
            return;
        const text = replyField.text.trim();
        if (text === "")
            return;
        // Cleared first: sendInlineReply may close the notification, and a field
        // cleared afterwards would be writing into a card whose bindings have
        // just gone null.
        replyField.text = "";
        card.notification.sendInlineReply(text);
        card.replied();
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
    //
    // Toasts only. A card in the centre is created every time the list is
    // rebuilt, which happens whenever the history changes, so fading in here
    // would flash the whole column each time anything arrived; the centre fades
    // its panel in once instead.
    NumberAnimation on opacity {
        running: card.compact
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
            // Toast only. A card in the centre wears its identity on the leading
            // tile and the top line of the text column instead, so this row
            // there would be a second app name over dead space.
            visible: card.compact

            AppIcon {
                Layout.preferredWidth: 18
                Layout.preferredHeight: 18
                Layout.alignment: Qt.AlignVCenter
                pixels: 36
            }

            Text {
                Layout.fillWidth: true
                visible: card.compact
                text: card.live ? String(card.notification.appName || "Notification") : ""
                color: Theme.textMuted
                font.family: Theme.sans
                font.pixelSize: 11
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                renderType: Text.NativeRendering
            }

            // Re-read on the shell's minute tick, which is the whole reason
            // the daemon owns a clock: a timer per card would wake the
            // process once for every visible row.
            TimeLabel {}
            CloseButton {}
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            // Centre only, and every card gets one. The centre used to draw a
            // single icon on a header above each group; the stack replaced the
            // header, so identity travels with the card -- which is also what
            // makes an expanded stack legible as one app's list.
            AppIcon {
                Layout.preferredWidth: 32
                Layout.preferredHeight: 32
                Layout.alignment: Qt.AlignTop
                visible: !card.compact
                pixels: 64
            }

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

                // The centre card's top line: who sent it, what is still stacked
                // underneath, how old it is, and the way out. Small and muted, so
                // it reads as a caption over the title rather than as a heading
                // competing with it.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    visible: !card.compact

                    Text {
                        Layout.fillWidth: true
                        text: card.live
                            ? String(card.notification.appName || "Notification") : ""
                        color: Theme.textMuted
                        font.family: Theme.sans
                        font.pixelSize: 11
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        renderType: Text.NativeRendering
                    }

                    Text {
                        text: card.stackNote
                        visible: card.stackNote !== ""
                        color: Theme.textMuted
                        font.family: Theme.sans
                        font.pixelSize: 11
                        font.weight: Font.Medium
                        renderType: Text.NativeRendering
                    }

                    TimeLabel {}
                    CloseButton {}
                }

                Text {
                    id: summaryText
                    Layout.fillWidth: true
                    text: card.live ? String(card.notification.summary || "") : ""
                    visible: summaryText.text !== ""
                    color: Theme.text
                    font.family: Theme.sans
                    font.pixelSize: 13
                    // Semibold in the list, where the top line above it is
                    // already demibold and a bold title would shout over a
                    // column of them. The toast keeps the weight it was
                    // approved with.
                    font.weight: card.compact ? Font.Bold : Font.DemiBold
                    wrapMode: Text.WordWrap
                    maximumLineCount: card.compact || card.collapsed ? 2 : 4
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
                    // A glance on a toast, a glance on top of a closed stack,
                    // and the whole thing once the row is one the user opened.
                    maximumLineCount: card.compact ? 4 : card.collapsed ? 2 : 12
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
            // Not off the top of a closed stack: a button there is a target the
            // click that was meant to open the stack would land on.
            visible: !card.collapsed && card.actions.length > 0
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

        // The reply field. Built from a TextInput in a shape rather than a
        // Controls TextField, like every other field in the shell: Controls would
        // bring its own style into a shell that draws all of its own.
        RowLayout {
            Layout.fillWidth: true
            visible: card.canReply
            spacing: 6

            ContinuousRectangle {
                Layout.fillWidth: true
                implicitHeight: 30
                radius: Theme.controlRadius
                color: Theme.hoverStrong
                borderWidth: 1
                // The focus ring is the only thing that says the keyboard is in
                // here rather than in the window behind the centre.
                borderColor: replyField.activeFocus ? Theme.accent : Theme.border

                TextInput {
                    id: replyField
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    verticalAlignment: Text.AlignVCenter
                    color: Theme.text
                    selectionColor: Theme.accent
                    selectedTextColor: Theme.accentText
                    font.family: Theme.sans
                    font.pixelSize: 12
                    selectByMouse: true
                    clip: true

                    // Return sends. Escape is deliberately not handled, so it
                    // reaches the surface and closes it: a field that swallowed
                    // Escape would leave the user with no way out but the mouse.
                    Keys.onReturnPressed: card.sendReply()
                    Keys.onEnterPressed: card.sendReply()

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: replyField.text === ""
                        // The sender's own placeholder when it sent one -- it is
                        // the only party that knows whether this is a chat, a
                        // comment or a subject line.
                        text: card.live
                            && String(card.notification.inlineReplyPlaceholder || "") !== ""
                            ? String(card.notification.inlineReplyPlaceholder) : "Reply"
                        color: Theme.textDisabled
                        font: replyField.font
                        renderType: Text.NativeRendering
                    }
                }
            }

            ContinuousRectangle {
                id: sendButton
                // Children of a ContinuousRectangle are reparented into its
                // content item, so they reach this through the id rather than
                // through parent.
                readonly property bool ready: replyField.text.trim() !== ""

                implicitWidth: 30
                implicitHeight: 30
                radius: Theme.controlRadius
                color: !sendButton.ready ? Theme.hover
                    : sendPointer.containsMouse ? Theme.accentHover : Theme.accent

                Image {
                    id: sendGlyph
                    anchors.centerIn: parent
                    width: 14
                    height: 14
                    source: "icons/arrow-up.svg"
                    sourceSize.width: 28
                    sourceSize.height: 28
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    antialiasing: true
                    mipmap: true
                    visible: false
                }

                ColorOverlay {
                    anchors.fill: sendGlyph
                    source: sendGlyph
                    color: sendButton.ready ? Theme.accentText : Theme.textDisabled
                    cached: true
                }

                MouseArea {
                    id: sendPointer
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: sendButton.ready
                        ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: card.sendReply()
                }
            }
        }
    }
}
