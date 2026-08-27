pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

// Isolated regression harness for a widget whose data arrives after the host loads.
// From the repository root, run with:
// qs -p desktop/.config/quickshell/garage/bar-module-visibility-test.qml
ShellRoot {
    id: testRoot

    property int stage: 0
    property int attempts: 0
    property bool finished: false

    QtObject {
        id: services

        property bool ready: false
    }

    QtObject {
        id: registry

        readonly property var entry: ({
            manifest: { id: "delayed" },
            widget: { inline: true, vertical: "show" },
            widgetUrl: Qt.resolvedUrl(
                "tests/bar-module-visibility/DelayedWidget.qml"),
            popupUrl: "",
            root: ""
        })

        function lookup(id) {
            return id === "delayed" ? entry : null;
        }
    }

    BarModule {
        id: module

        extensionId: "delayed"
        registry: registry
        services: services
        screen: ({ name: "test" })
        screenName: "test"
    }

    function finish(success, message) {
        if (finished)
            return;
        finished = true;
        console.log(success ? "GARAGE_TEST_PASS" : "GARAGE_TEST_FAIL", message);
        quitTimer.start();
    }

    Timer {
        interval: 25
        running: true
        repeat: true
        onTriggered: {
            ++testRoot.attempts;
            if (testRoot.attempts > 80) {
                testRoot.finish(false, "timed out waiting for the widget state");
                stop();
                return;
            }

            if (testRoot.stage === 0) {
                if (module.contentVisible)
                    return;
                if (!module.visible || module.width !== 0) {
                    testRoot.finish(false,
                        "the empty widget hid its host or retained layout space");
                    stop();
                    return;
                }
                testRoot.stage = 1;
                testRoot.attempts = 0;
                services.ready = true;
                return;
            }

            if (!module.contentVisible)
                return;
            if (!module.visible || module.width < module.thickness) {
                testRoot.finish(false,
                    "the ready widget did not reclaim its layout space");
                stop();
                return;
            }
            testRoot.finish(true,
                "a delayed widget became visible without recreating its host");
            stop();
        }
    }

    Timer {
        id: quitTimer

        interval: 50
        onTriggered: Qt.quit()
    }
}
