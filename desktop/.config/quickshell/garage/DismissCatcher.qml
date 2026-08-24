import Quickshell
import Quickshell.Wayland
import QtQuick

// Click-outside dismissal for the transient palettes, as a transparent layer
// surface under them rather than as a focus grab.
//
// The shell used to ask for HyprlandFocusGrab and let the compositor report the
// click. This compositor does not implement hyprland-focus-grab at all -- the
// shell log says `Blackholed import URL qs:@/Quickshell/Hyprland/_FocusGrab`,
// which is quickshell saying the protocol is missing -- so every grab in the
// shell was inert and onCleared never fired. Nothing was wrong with the wiring;
// there was simply nothing on the other end of it.
//
// What does work here is the shape the session confirmation dialog already
// uses: a second surface, the size of the screen, sitting underneath the thing
// it guards and turning a click anywhere else into a dismissal. That is all
// this is, made shared and put on every monitor -- clicking on any screen
// dismisses, the way a menu on one macOS display is dismissed by a click on
// another.
//
// Two flags rather than one, and the difference between them is the whole
// reason this file has any subtlety in it:
//
//   active   -- whether the catcher exists as a mapped surface.
//   armed    -- whether it covers the screen and dismisses on a click.
//
// Layer-shell surfaces on one layer stack in the order they are mapped, and the
// palette has to stay above the catcher: a catcher above it would eat the
// clicks meant for the panel and dismiss on every one of them. Measured on this
// compositor, with the catcher declared before the palette loaders in shell.qml
// and both surfaces shown in the same turn, the palette maps second and lands
// on top -- and it stays on top across a switch from one palette to another and
// across a full close-and-reopen.
//
// What does *not* survive is unmapping the catcher on its own and bringing it
// back while the palette is still up: measured, it comes back at the top of the
// overlay stack, over the panel. That is exactly what the screenshot flow would
// do to it -- the pill goes up, the catcher stands down, the capture ends, the
// catcher returns -- so standing down cannot mean unmapping. It means keeping
// the surface mapped where it is and collapsing it to a corner pixel, which is
// `armed` below: same surface, same place in the stack, no longer in the way of
// the pill or of slurp's region select.
Scope {
    id: catcher

    // Set while one of the palettes this guards is on screen.
    property bool active: false

    // Cleared while another shell surface owns the interaction -- the
    // screenshot pill and the capture it starts. See the note above: this
    // collapses the catcher rather than taking it down.
    property bool armed: true

    signal dismissed()

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            // Anchored on all four sides when it is doing its job, and on two
            // when it is standing down, which leaves it at its implicit 1x1 in
            // the top-left corner of the output. Collapsing by geometry rather
            // than by visibility is what keeps it underneath the palette.
            readonly property bool covering: catcher.active && catcher.armed

            screen: modelData
            visible: catcher.active
            color: "transparent"
            // No keyboard, on either the generic or the layer-shell property:
            // the launcher is an on-demand keyboard surface and has to keep the
            // keystrokes it is being typed into.
            focusable: false
            aboveWindows: true
            // Ignore rather than zero: a surface that respects the bar's
            // exclusive zone stops below the bar, and a click on the bar would
            // then not dismiss. With Ignore the surface is handed the whole
            // output -- measured at the monitor origin, full height -- so no
            // negative margins are needed to reach back over the bar.
            exclusionMode: ExclusionMode.Ignore
            surfaceFormat.opaque: false

            implicitWidth: 1
            implicitHeight: 1

            anchors {
                left: true
                top: true
                right: covering
                bottom: covering
            }

            WlrLayershell.layer: WlrLayer.Overlay
            // Deliberately absent from every blur, glass and no_screen_share
            // rule in windowrules.lua and decorations.lua: those match layer
            // namespaces exactly, and there is nothing here to blur or to hide
            // from a screen share.
            WlrLayershell.namespace: "garage-dismiss-catcher"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

            MouseArea {
                anchors.fill: parent
                // Any button, the way a menu is dismissed by a right-click
                // outside it as readily as by a left-click.
                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                // On press rather than on click: a menu goes away as the button
                // goes down. It also means the release of the click that opened
                // the palette -- which lands here, because this surface maps
                // between that press and that release -- cannot be read as a
                // dismissal, since the press that would pair with it went to
                // the bar.
                onPressed: {
                    if (catcher.armed)
                        catcher.dismissed();
                }
            }
        }
    }
}
