pragma Singleton
import Quickshell
import Quickshell.Io
import QtQuick

// Discovers both extension roots through one bounded process. The emitter walks
// shipped first and user second; assigning by id makes the user copy win without
// one process or watcher per manifest. Discovery is intentionally startup-only.
Singleton {
    id: registry

    property var entries: ({})
    property var pending: ({})
    property int revision: 0
    readonly property var barWidgets: {
        const ignored = revision;
        return Object.values(entries).map(entry => ({
            id: entry.id,
            name: String(entry.manifest.name || entry.id),
            icon: entry.widget.icon,
            defaultAnchor: entry.widget.defaultAnchor,
            vertical: entry.widget.vertical
        })).sort((left, right) => left.id.localeCompare(right.id));
    }

    function lookup(id) {
        const ignored = revision;
        return entries[String(id)] || null;
    }

    function validId(value) {
        return typeof value === "string"
            && /^[a-z0-9][a-z0-9-]*$/.test(value);
    }

    function safeRelative(value) {
        if (typeof value !== "string" || value === "" || value.startsWith("/"))
            return false;
        return value.split("/").indexOf("..") < 0;
    }

    function consume(line) {
        let value;
        try {
            value = JSON.parse(String(line));
        } catch (error) {
            return;
        }
        const root = String(value._garage_root || "");
        delete value._garage_root;
        const directoryId = root.split("/").pop();
        if (value === null || typeof value !== "object"
                || !validId(value.id) || value.id !== directoryId || root === "")
            return;
        if (!Array.isArray(value.provides)
                || value.provides.indexOf("bar-widget") < 0)
            return;
        const declaredWidget = value["bar-widget"];
        const widget = Object.assign({}, declaredWidget || ({}), {
            defaultAnchor: declaredWidget && ["left", "center", "right"]
                .indexOf(declaredWidget.defaultAnchor) >= 0
                ? declaredWidget.defaultAnchor : "right",
            vertical: declaredWidget && declaredWidget.vertical === "hide"
                ? "hide" : "show"
        });
        if (declaredWidget === null || typeof declaredWidget !== "object"
                || !safeRelative(widget.icon)
                || ["left", "center", "right"].indexOf(widget.defaultAnchor) < 0)
            return;
        if (widget.popup !== undefined && !safeRelative(widget.popup))
            return;
        if (widget.surface !== undefined
                && (typeof widget.surface !== "string" || widget.surface === ""))
            return;
        if (widget.inline !== undefined && typeof widget.inline !== "boolean")
            return;
        if (widget.popup !== undefined && widget.surface !== undefined)
            return;
        if (value.probe !== undefined) {
            if (value.probe === null || typeof value.probe !== "object"
                    || !Array.isArray(value.probe.command)
                    || value.probe.command.length === 0)
                return;
            for (const argument of value.probe.command) {
                if (typeof argument !== "string" || argument === "")
                    return;
            }
            if (value.probe.restartMs !== undefined
                    && (typeof value.probe.restartMs !== "number"
                        || value.probe.restartMs < 250))
                return;
        }
        const next = Object.assign({}, pending);
        next[value.id] = {
            id: value.id,
            root: root,
            manifest: value,
            widget: widget,
            widgetUrl: root + "/Widget.qml",
            probe: value.probe || null,
            popupUrl: typeof widget.popup === "string"
                ? root + "/" + widget.popup : "",
            hasProbe: value.probe !== undefined
        };
        pending = next;
    }

    Process {
        id: discover
        running: true
        command: ["sh", "-c", String.raw`
set -eu
for extension_root in "$1" "$2"; do
    [ -d "$extension_root" ] || continue
    # Stow exposes shipped manifests as symlinks, while user manifests are
    # normally regular files. -xtype f accepts both without accepting a broken
    # link or a directory named manifest.json.
    find "$extension_root" -mindepth 2 -maxdepth 2 -name manifest.json -xtype f -print |
        LC_ALL=C sort |
        while IFS= read -r manifest; do
            root=$(dirname -- "$manifest")
            jq -c --arg root "$root" '. + {"_garage_root": $root}' "$manifest" || true
        done
done
`, "garage-extension-scan", GaragePaths.shippedExtensions,
            GaragePaths.userExtensions]

        onStarted: registry.pending = ({})
        stdout: SplitParser {
            splitMarker: "\n"
            onRead: line => registry.consume(line)
        }
        onExited: exitCode => {
            if (exitCode !== 0)
                console.warn("Garage extension discovery exited", exitCode);
            registry.entries = registry.pending;
            ++registry.revision;
        }
    }
}
