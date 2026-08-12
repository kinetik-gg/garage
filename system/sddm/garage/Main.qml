import QtQuick 2.15

Rectangle {
    id: page

    width: 1280
    height: 720
    color: "#161617"

    property int sessionIndex: sessionModel.lastIndex >= 0 &&
                               sessionModel.lastIndex < sessionModel.count ?
                               sessionModel.lastIndex : 0
    property string sessionName: "Desktop session"
    property string username: userModel.lastUser
    property bool switchingUser: username.length === 0
    property bool authenticating: false
    property string statusText: ""
    property bool statusIsError: false
    property string pendingPower: ""

    signal tryLogin()

    FontLoader {
        id: garageFont
        source: "PlusJakartaSans.ttf"
    }

    function submit() {
        if (authenticating)
            return
        username = switchingUser ? usernameField.text.trim() : username
        if (username.length === 0) {
            statusIsError = true
            statusText = "Enter a user name"
            usernameField.takeFocus()
            return
        }
        statusIsError = false
        statusText = "Authenticating…"
        authenticating = true
        sessionMenu.visible = false
        sddm.login(username, passwordField.text, sessionIndex)
    }

    function chooseSession(chosenIndex, chosenName) {
        sessionIndex = chosenIndex
        sessionName = chosenName
        sessionMenu.visible = false
    }

    function requestPower(action) {
        sessionMenu.visible = false
        if (action === "sleep") {
            if (sddm.canSuspend)
                sddm.suspend()
            return
        }
        pendingPower = action
    }

    function confirmPower() {
        if (pendingPower === "reboot" && sddm.canReboot)
            sddm.reboot()
        else if (pendingPower === "shutdown" && sddm.canPowerOff)
            sddm.powerOff()
        pendingPower = ""
    }

    Connections {
        target: sddm

        function onLoginSucceeded() {
            page.statusIsError = false
            page.statusText = "Starting session…"
        }

        function onLoginFailed() {
            page.authenticating = false
            page.statusIsError = true
            page.statusText = "The password was not accepted"
            passwordField.text = ""
            passwordField.takeFocus()
        }

        function onInformationMessage(message) {
            if (message.length === 0)
                return
            page.statusText = message
            page.statusIsError = false
        }
    }

    Image {
        anchors.fill: parent
        source: config.stringValue("background")
        fillMode: Image.PreserveAspectCrop
        asynchronous: false
        cache: true
    }

    Rectangle {
        anchors.fill: parent
        color: "#73000000"
    }

    Item {
        id: primarySurface
        anchors.fill: parent
        visible: primaryScreen

        Rectangle {
            id: card
            width: Math.min(430, parent.width - 48)
            height: 490
            anchors.centerIn: parent
            radius: 22
            color: "#ed1c1c1e"
            border.width: 1
            border.color: "#293a3a3c"

            Column {
                id: content
                anchors.fill: parent
                anchors.margins: 28
                spacing: 12

                Text {
                    width: parent.width
                    height: 34
                    text: "Welcome back"
                    color: "#f5f5f7"
                    font.family: garageFont.name
                    font.pixelSize: 25
                    font.weight: Font.DemiBold
                    verticalAlignment: Text.AlignVCenter
                }

                Item {
                    width: parent.width
                    height: 42

                    Text {
                        anchors.left: parent.left
                        anchors.right: switchUser.left
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !page.switchingUser
                        text: page.username
                        color: "#d1d1d6"
                        elide: Text.ElideRight
                        font.family: garageFont.name
                        font.pixelSize: 14
                    }

                    GarageField {
                        id: usernameField
                        anchors.left: parent.left
                        anchors.right: switchUser.left
                        anchors.rightMargin: 8
                        height: 42
                        visible: page.switchingUser
                        placeholder: "User name"
                        fontFamily: garageFont.name
                        text: page.username
                        onAccepted: passwordField.takeFocus()
                    }

                    GarageAction {
                        id: switchUser
                        width: 92
                        height: 42
                        anchors.right: parent.right
                        ghost: true
                        fontFamily: garageFont.name
                        label: page.switchingUser ? "Use user" : "Switch user"
                        onClicked: {
                            if (page.switchingUser) {
                                var candidate = usernameField.text.trim()
                                if (candidate.length === 0) {
                                    page.statusIsError = true
                                    page.statusText = "Enter a user name"
                                    usernameField.takeFocus()
                                    return
                                }
                                page.username = candidate
                            }
                            page.switchingUser = !page.switchingUser
                            if (page.switchingUser) {
                                usernameField.text = page.username
                                usernameField.takeFocus()
                                usernameField.selectAll()
                            } else {
                                passwordField.takeFocus()
                            }
                        }
                    }
                }

                Text {
                    width: parent.width
                    height: 18
                    text: "Password"
                    color: "#a8a8ad"
                    font.family: garageFont.name
                    font.pixelSize: 12
                    font.weight: Font.Medium
                    verticalAlignment: Text.AlignBottom
                }

                GarageField {
                    id: passwordField
                    width: parent.width
                    height: 46
                    secret: true
                    readOnly: page.authenticating
                    placeholder: "Enter your password"
                    fontFamily: garageFont.name
                    onAccepted: page.submit()
                }

                Text {
                    id: statusLabel
                    width: parent.width
                    height: 22
                    text: page.statusText
                    color: page.statusIsError ? "#ff6961" : "#a8a8ad"
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                    font.family: garageFont.name
                    font.pixelSize: 12
                }

                GarageAction {
                    id: loginButton
                    width: parent.width
                    height: 46
                    primary: true
                    busy: page.authenticating
                    enabled: !page.authenticating
                    fontFamily: garageFont.name
                    label: "Sign in"
                    onClicked: page.submit()
                }

                Item {
                    width: parent.width
                    height: 46
                    z: 10

                    Rectangle {
                        anchors.fill: parent
                        radius: 12
                        color: sessionPointer.containsMouse || sessionToggle.activeFocus ?
                               "#2c2c2e" : "#202022"
                        border.width: 1
                        border.color: "#293a3a3c"
                    }

                    Text {
                        anchors.left: parent.left
                        anchors.right: sessionChevron.left
                        anchors.leftMargin: 14
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: page.sessionName
                        color: "#d1d1d6"
                        elide: Text.ElideRight
                        font.family: garageFont.name
                        font.pixelSize: 13
                    }

                    Image {
                        id: sessionChevron
                        width: 18
                        height: 18
                        anchors.right: parent.right
                        anchors.rightMargin: 14
                        anchors.verticalCenter: parent.verticalCenter
                        source: "icons/chevron.svg"
                        rotation: sessionMenu.visible ? 180 : 0
                        sourceSize.width: 36
                        sourceSize.height: 36
                    }

                    FocusScope {
                        id: sessionToggle
                        anchors.fill: parent
                        activeFocusOnTab: true
                    }

                    MouseArea {
                        id: sessionPointer
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: sessionMenu.visible = !sessionMenu.visible
                    }

                    Rectangle {
                        id: sessionMenu
                        width: parent.width
                        height: Math.max(42, sessionModel.count * 40 + 8)
                        anchors.left: parent.left
                        anchors.bottom: parent.top
                        anchors.bottomMargin: 6
                        visible: false
                        radius: 12
                        color: "#fa242426"
                        border.width: 1
                        border.color: "#3a3a3c"
                        z: 30

                        Column {
                            anchors.fill: parent
                            anchors.margins: 4

                            Repeater {
                                model: sessionModel

                                delegate: Rectangle {
                                    required property int index
                                    required property string name

                                    width: sessionMenu.width - 8
                                    height: 40
                                    radius: 9
                                    color: sessionRow.containsMouse || index === page.sessionIndex ?
                                           "#2c2c2e" : "transparent"

                                    Component.onCompleted: {
                                        if (index === page.sessionIndex)
                                            page.sessionName = name
                                    }

                                    Text {
                                        anchors.fill: parent
                                        anchors.leftMargin: 10
                                        anchors.rightMargin: 10
                                        text: name
                                        color: "#d1d1d6"
                                        elide: Text.ElideRight
                                        verticalAlignment: Text.AlignVCenter
                                        font.family: garageFont.name
                                        font.pixelSize: 13
                                    }

                                    MouseArea {
                                        id: sessionRow
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: page.chooseSession(index, name)
                                    }
                                }
                            }
                        }
                    }
                }

                Item {
                    width: parent.width
                    height: 50

                    Row {
                        anchors.fill: parent
                        spacing: 8
                        visible: page.pendingPower === ""

                        GarageAction {
                            width: (parent.width - 16) / 3
                            height: parent.height
                            label: "Sleep"
                            iconSource: "icons/sleep.svg"
                            fontFamily: garageFont.name
                            enabled: sddm.canSuspend
                            onClicked: page.requestPower("sleep")
                        }

                        GarageAction {
                            width: (parent.width - 16) / 3
                            height: parent.height
                            label: "Reboot"
                            iconSource: "icons/reboot.svg"
                            fontFamily: garageFont.name
                            enabled: sddm.canReboot
                            onClicked: page.requestPower("reboot")
                        }

                        GarageAction {
                            width: (parent.width - 16) / 3
                            height: parent.height
                            label: "Shut down"
                            iconSource: "icons/power.svg"
                            fontFamily: garageFont.name
                            enabled: sddm.canPowerOff
                            onClicked: page.requestPower("shutdown")
                        }
                    }

                    Row {
                        anchors.fill: parent
                        spacing: 8
                        visible: page.pendingPower !== ""

                        Text {
                            width: parent.width - 210
                            height: parent.height
                            text: page.pendingPower === "reboot" ? "Reboot this computer?" :
                                                                  "Shut down this computer?"
                            color: "#d1d1d6"
                            elide: Text.ElideRight
                            verticalAlignment: Text.AlignVCenter
                            font.family: garageFont.name
                            font.pixelSize: 13
                        }

                        GarageAction {
                            width: 96
                            height: parent.height
                            ghost: true
                            label: "Cancel"
                            fontFamily: garageFont.name
                            onClicked: page.pendingPower = ""
                        }

                        GarageAction {
                            width: 106
                            height: parent.height
                            primary: true
                            destructive: true
                            label: page.pendingPower === "reboot" ? "Reboot" : "Shut down"
                            fontFamily: garageFont.name
                            onClicked: page.confirmPower()
                        }
                    }
                }
            }
        }
    }

    Component.onCompleted: {
        if (primaryScreen) {
            if (page.switchingUser)
                usernameField.takeFocus()
            else
                passwordField.takeFocus()
        }
    }
}
