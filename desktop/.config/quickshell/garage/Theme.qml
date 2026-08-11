pragma Singleton

import Quickshell
import Quickshell.Io
import QtQuick

QtObject {
    id: theme

    readonly property int windowGutter: 6

    // Corner geometry, mirroring the compositor's decoration:rounding and
    // rounding_power. ContinuousRectangle draws the same superellipse Hyprland
    // clips windows with, so the shell's silhouettes only stay aligned with the
    // glass behind them while these match. Pushed from the signal for the same
    // reason as scheme below.
    property int cornerRadius: 7
    property FileView cornerRadiusFile: FileView {
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/corner-radius"
        printErrors: false
        watchChanges: true
        // parseInt("0") is falsy, and the square setting is a real value rather
        // than a missing file, so this cannot fall back through ||.
        onFileChanged: reload()
        onLoaded: {
            const parsed = parseInt(String(text()).trim());
            theme.cornerRadius = isNaN(parsed) ? 7 : parsed;
        }
    }
    readonly property real cornerPower: 3.37

    // Reduce Motion, for the animations Hyprland's own switch cannot reach.
    // Every palette in the shell animates its own entrance -- see PanelMotion --
    // and a compositor layer rule is not what is driving those, so the setting
    // has to arrive here as well. Off means the surface is simply there.
    property bool reduceMotion: false
    property FileView reduceMotionFile: FileView {
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/reduce-motion"
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: theme.reduceMotion = String(text()).trim() === "1"
    }

    // Radius for buttons, list rows, fields and other small controls.
    //
    // ContinuousRectangle draws radius * cornerPower / 2 and clamps at half the
    // shape, so a ~32 px control saturates at 15.5 px: handed the window radius
    // it rendered the same pill at every setting above the smallest, which is
    // why the setting looked like it never reached a button. The fraction keeps
    // the three settings distinct on a control (0 / 7.1 / 10.1 px visible); the
    // cap is the largest radius that still leaves straight edge on the shortest
    // control in the shell, the 22 px switch, so nothing becomes a pill.
    readonly property real controlRadius: Math.min(cornerRadius * 0.6, 6)

    // An inner shape inset from its parent by `inset` px stays concentric only
    // if its visible radius is smaller by exactly that much, so the subtraction
    // has to be un-amplified before it is applied. One convention for every
    // nested silhouette in the shell; clamped at zero so the square setting
    // stays square instead of going negative.
    function insetRadius(radius, inset) {
        return Math.max(0, radius - inset * 2 / cornerPower);
    }

    // Written by garage, which resolves light/dark/auto.
    //
    // The value is pushed into a plain property from the signals rather than
    // bound through text(). text() is a function that also assigns several of
    // FileView's own properties as a side effect, so a binding built on it does
    // not reliably re-evaluate when the file changes -- the shell would repaint
    // only when a surface was recreated. Everything in the shell reads Theme, so
    // this has to be dependable before more of it is built.
    property string scheme: "dark"
    property FileView schemeFile: FileView {
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/color-scheme"
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: theme.scheme = String(text()).trim() || "dark"
    }
    readonly property bool dark: scheme !== "light"

    // The window material. When it is off no glass is drawn behind the shell,
    // and decoration opacity multiplies client alpha rather than adding to it,
    // so a translucent surface would have nothing underneath it -- the panels
    // and the session menu would simply have no background. Every token that
    // leans on the material for its body goes opaque instead.
    property string material: "liquid"
    property FileView materialFile: FileView {
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/material"
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: theme.material = String(text()).trim() || "liquid"
    }
    readonly property bool solid: material === "off"

    // Body colours the tinted tokens are built from, so the opaque and the
    // translucent forms cannot drift apart.
    readonly property color bodyBase: dark ? "#1c1c1e" : "#f5f5f7"
    readonly property color bodyRaised: dark ? "#2c2c2e" : "#ffffff"

    // Every surface in the shell reads these tokens rather than literal colours,
    // so a palette is the only thing a new appearance needs to define.
    readonly property color surface: solid ? bodyBase : (dark ? "#f21c1c1e" : "#f2f5f5f7")
    readonly property color surfaceRaised: solid ? bodyRaised : (dark ? "#f22c2c2e" : "#f2ffffff")
    // Window background for the layer surfaces listed in the compositor's
    // kinetik_glass layer_namespaces. Those get the glass material drawn beneath
    // them, so painting a surface here would cover it -- the same mistake the
    // terminal was making with its own background.
    readonly property color panel: solid ? bodyBase : "transparent"
    // The body every floating surface in the shell wears, over the glass and
    // under the content. `panel` above is transparent so the compositor's
    // material shows through, and glass alone is not a surface anything can be
    // read on: over a white browser page a panel with no body of its own washes
    // out completely, and the text with it.
    //
    // One value, not one per kind of surface. This used to be four -- a panel
    // tint at 90%, dialogs and the screenshot pill at 75%, menus at 25% -- and
    // what a surface floats over has nothing to do with whether it is a menu or
    // a dialog. They all float over the same arbitrary desktop, so they all need
    // the same body, and four of them only meant three of the shell's surfaces
    // were unreadable in different ways.
    readonly property color contentTint: solid ? bodyRaised : (dark ? "#e62c2c2e" : "#e6ffffff")
    // Icon well in dialogs: deliberately darker than the surface it sits on, so
    // the glyph can be light and carry the contrast itself.
    readonly property color iconWell: dark ? "#59000000" : "#59000000"
    readonly property color iconWellGlyph: "#fff5f5f7"
    readonly property color border: dark ? "#2bffffff" : "#2b000000"
    readonly property color frameOuter: dark ? "#26000000" : "#26000000"
    readonly property color frameInner: dark ? "#1affffff" : "#1a000000"
    readonly property color text: dark ? "#fff5f5f7" : "#ff0b0b0d"
    readonly property color textMuted: dark ? "#ffa8a8ad" : "#ff45454a"
    readonly property color textDisabled: dark ? "#ff6e6e73" : "#ff8a8a90"
    readonly property color hover: dark ? "#0dffffff" : "#0d000000"
    readonly property color hoverStrong: dark ? "#18ffffff" : "#18000000"
    // Recessed sidebar well, modal scrim, and the slider knob. All three were
    // literals that assumed a dark backdrop.
    readonly property color sidebarWell: solid ? (dark ? "#ff111113" : "#ffffffff")
        : (dark ? "#bf111113" : "#bfffffff")
    readonly property color scrim: dark ? "#40000000" : "#40000000"
    readonly property color knob: dark ? "#ffe5e5e7" : "#ffffffff"

    // The nine choosable accents, and the shell's only copy of a palette that
    // otherwise lives once in the backend's PALETTE/ACCENTS. QML can neither
    // @import a stylesheet nor read a Python mapping, and the accent marker
    // carries the *name* rather than the colour, so the hexes have to be here.
    // tests/test_palette.py fails if they drift from ACCENTS.
    readonly property var accentPalette: ({
        blue: "#ff3584e4", teal: "#ff2190a4", green: "#ff3a944a",
        yellow: "#fff6d32d", orange: "#ffff7800", red: "#ffe01b24",
        pink: "#ffd56199", purple: "#ff9141ac", slate: "#ff6f8396"
    })
    // The order the Appearance pane draws the swatch row in. Derived rather than
    // listed a second time: an accent present in one and missing from the other
    // is either a swatch with no colour or a colour nobody can pick.
    readonly property var accentNames: Object.keys(accentPalette)
    property FileView accentFile: FileView {
        path: Quickshell.env("HOME") + "/.local/state/garage/generated/accent"
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: theme.accentName = String(text()).trim() || "blue"
    }
    property string accentName: "blue"
    readonly property color accent: accentPalette[accentName] || accentPalette.blue
    readonly property color accentSoft: accent
    // Lightening a saturated accent on a light surface washes it out, so hover
    // goes the other way there.
    readonly property color accentHover: dark ? Qt.lighter(accent, 1.12) : Qt.darker(accent, 1.12)
    readonly property color accentText: accentName === "yellow" ? "#ff1c1c1e" : "#ffffffff"

    readonly property string sans: "Plus Jakarta Sans"
    readonly property string mono: "Geist Mono"
}
