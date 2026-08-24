pragma Singleton
import Quickshell
import QtQuick

// Where the backend lives, written down once.
//
// A dozen files each rebuilt "$HOME/.local/bin/..." from Quickshell.env, which
// meant a dozen places to edit the day the install layout moves. Every command
// the shell spawns names its binary through here instead.
QtObject {
    readonly property string home: Quickshell.env("HOME")
    readonly property string binDir: home + "/.local/bin"
    readonly property string stateDir: home + "/.local/state/garage/generated"
    readonly property string shellDir: Quickshell.shellDir

    readonly property string garage: binDir + "/garage"
    readonly property string screenshotCopy: binDir + "/garage-screenshot-copy"
    readonly property string fileIndex: binDir + "/garage-file-index"
    readonly property string vramInfo: binDir + "/garage-vram-info"
    readonly property string aiUsage: binDir + "/garage-ai-usage"
    readonly property string barProbe: binDir + "/garage-bar-probe"
    readonly property string metrics: binDir + "/garage-metrics"

    readonly property string barLayout: stateDir + "/bar-layout.json"
    readonly property string clockFormat: stateDir + "/clock-format.json"
    readonly property string launcherMode: stateDir + "/launcher"

    // Shipped widgets live with the shell; third-party widgets live beside
    // other user data. ExtensionRegistry resolves both and lets the user root
    // win an id collision.
    readonly property string shippedExtensions: shellDir + "/extensions"
    readonly property string userExtensions: home + "/.local/share/garage/extensions"
}
