import QtQuick
import QtQuick.Layouts

Flickable {
    id: pane
    required property var controller
    contentHeight: content.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // The accents come from Theme, which is the shell's single copy of them.
    // This pane kept its own list and its own hexes until the palette was
    // consolidated; two copies in the same process meant the swatch row could
    // offer a colour the rest of the shell would not draw.
    readonly property real borderSize: pane.controller.preference(
        "appearance", "border_size", 0)
    readonly property bool reduceMotion: pane.controller.preference(
        "appearance", "reduce_motion", false)
    readonly property real motionSpeed: pane.controller.preference(
        "appearance", "animation_speed", 1)

    readonly property string glassMode: pane.controller.preference(
        "appearance", "glass_mode", "liquid")
    // One line per style rather than all three at once: describing every style
    // in every state wrapped the label column to four lines, which is the whole
    // of what made this group read as cramped.
    readonly property var glassBlurbs: ({
        liquid: "Refracts the desktop at window edges.",
        frosted: "Blurs the desktop without bending it.",
        off: "Solid windows, no blur or transparency."
    })

    function titleCase(value) {
        return value.charAt(0).toUpperCase() + value.slice(1);
    }

    // The slider is a speed, but Hyprland's own number is a duration, so the
    // helper divides rather than multiplies. Spell out which way the desktop
    // moves instead of leaving a bare multiplier the label could read either way.
    function motionNote(factor) {
        if (factor === 1)
            return "Normal";
        return Number(factor).toFixed(2).replace(/\.?0+$/, "") + "× — "
            + (factor > 1 ? "shorter" : "longer") + " animations";
    }

    ColumnLayout {
        id: content
        width: pane.width
        spacing: 22

        SettingsGroup {
            title: "COLORS"

            SettingsRow {
                title: "Accent Color"
                description: pane.titleCase(pane.controller.preference(
                    "appearance", "accent_color", "blue"))

                Row {
                    spacing: 7

                    Repeater {
                        model: Theme.accentNames

                        Rectangle {
                            required property string modelData
                            width: 22
                            height: 22
                            radius: width / 2
                            color: Theme.accentPalette[modelData]
                            border.width: pane.controller.preference(
                                "appearance", "accent_color", "blue") === modelData ? 2 : 0
                            border.color: Theme.text
                            antialiasing: true

                            Rectangle {
                                anchors.centerIn: parent
                                width: 6
                                height: 6
                                radius: width / 2
                                visible: parent.border.width > 0
                                color: modelData === "yellow" ? Theme.surface : Theme.text
                                antialiasing: true
                            }

                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: pane.controller.setPreference(
                                    "appearance", "accent_color", modelData)
                            }
                        }
                    }
                }
            }

        }

        SettingsGroup {
            title: "APPEARANCE"

            SettingsRow {
                title: "Appearance"
                description: "Auto follows the schedule below."
                SettingsSegmented {
                    model: ["Light", "Dark", "Auto"]
                    currentIndex: Math.max(0, ["light", "dark", "auto"].indexOf(
                        pane.controller.preference("appearance", "theme_mode", "dark")))
                    onActivated: index => pane.controller.setPreference(
                        "appearance", "theme_mode", ["light", "dark", "auto"][index])
                }
            }

            // One row per time rather than a pair of bare pickers: side by side
            // they gave no clue which was which, and four combos squeezed the
            // label column into a four-line wrap.
            SettingsRow {
                title: "Switch To Light At"
                description: "Times use the system's local clock."
                visible: pane.controller.preference("appearance", "theme_mode", "dark") === "auto"
                TimeField {
                    value: pane.controller.preference("appearance", "theme_light_at", "07:00")
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "theme_light_at", next)
                }
            }

            SettingsRow {
                title: "Switch To Dark At"
                visible: pane.controller.preference("appearance", "theme_mode", "dark") === "auto"
                TimeField {
                    value: pane.controller.preference("appearance", "theme_dark_at", "19:00")
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "theme_dark_at", next)
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Corner Radius"
                description: "Rounding for windows, panels, menus, and controls."
                SettingsSegmented {
                    // Keys and labels are positional, so the two lists have to
                    // stay the same length and order.
                    readonly property var radiusKeys: ["none", "normal", "large"]
                    model: ["None", "Normal", "Large"]
                    currentIndex: Math.max(0, radiusKeys.indexOf(
                        pane.controller.preference("appearance", "corner_radius", "normal")))
                    onActivated: index => pane.controller.setPreference(
                        "appearance", "corner_radius", radiusKeys[index])
                }
            }

            // Zero by default, which is what the desktop shipped with: the
            // material's own frame rings already mark the window edge, and the
            // compositor border sits outside them rather than beside them, so
            // any width reads as one thicker ring instead of two.
            SettingsRow {
                title: "Window Border"
                description: pane.borderSize === 0 ? "No border"
                    : Math.round(pane.borderSize) + " px"
                SettingsSlider {
                    from: 0
                    to: 10
                    stepSize: 1
                    value: pane.borderSize
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "border_size", Math.round(next))
                }
            }
        }

        SettingsGroup {
            title: "MOTION"

            // Off outright, not slowed: the only things left animated here are
            // movements, and a shorter slide is still a slide. Shortening is
            // what the speed control below already does.
            SettingsRow {
                title: "Reduce Motion"
                description: "Turn off workspace, panel, and window animations."
                SettingsSwitch {
                    checked: pane.reduceMotion
                    onToggled: value => pane.controller.setPreference(
                        "appearance", "reduce_motion", value)
                }
            }

            MenuSeparator {
                Layout.fillWidth: true
                visible: !pane.reduceMotion
            }

            SettingsRow {
                title: "Animation Speed"
                description: pane.motionNote(pane.motionSpeed)
                visible: !pane.reduceMotion
                SettingsSlider {
                    from: 0.5
                    to: 2
                    stepSize: 0.25
                    value: pane.motionSpeed
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "animation_speed", next)
                }
            }
        }

        SettingsGroup {
            title: "WINDOW MATERIAL"

            SettingsRow {
                title: "Style"
                description: pane.glassBlurbs[pane.glassMode]
                // Segmented rather than a dropdown: three mutually exclusive
                // styles that change which controls below apply, so seeing all
                // three at once beats saving the width.
                SettingsSegmented {
                    model: ["Liquid", "Frosted", "Off"]
                    currentIndex: Math.max(0, ["liquid", "frosted", "off"].indexOf(pane.glassMode))
                    onActivated: index => pane.controller.setPreference(
                        "appearance", "glass_mode", ["liquid", "frosted", "off"][index])
                }
            }

            MenuSeparator {
                Layout.fillWidth: true
                visible: pane.glassMode !== "off"
            }

            // Three steps rather than a slider: the depth is a pass count and a
            // buffer scale, and between them there are only about three visibly
            // distinct settings. Normal is what ran before this control existed.
            SettingsRow {
                title: "Blur"
                description: "Softness of the desktop behind glass."
                visible: pane.glassMode !== "off"
                SettingsSegmented {
                    model: ["Light", "Normal", "Deep"]
                    currentIndex: Math.max(0, ["light", "normal", "deep"].indexOf(
                        pane.controller.preference("appearance", "glass_blur", "normal")))
                    onActivated: index => pane.controller.setPreference(
                        "appearance", "glass_blur", ["light", "normal", "deep"][index])
                }
            }

            SettingsRow {
                title: "Transparency"
                description: Math.round(pane.controller.preference(
                    "appearance", "glass_transparency", 0.04) * 100) + "%"
                visible: pane.glassMode !== "off"
                // Capped well short of invisible: past a quarter the desktop
                // starts reading through body text.
                SettingsSlider {
                    from: 0
                    to: 0.25
                    stepSize: 0.01
                    value: pane.controller.preference("appearance", "glass_transparency", 0.04)
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "glass_transparency", next)
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            // Kept in this group rather than the edge one below so it survives
            // Off, where it is the only way back to a glass style.
            SettingsRow {
                title: "Reset"
                description: "Restore the material settings to their defaults."
                SettingsButton {
                    text: "Reset"
                    onClicked: pane.controller.action("glass.reset")
                }
            }
        }

        // Split off so neither group is a wall of sliders. Every control here
        // lives on the bevel, which Off does not draw at all.
        SettingsGroup {
            title: "GLASS EDGE"
            visible: pane.glassMode !== "off"

            SettingsRow {
                title: "Edge Thickness"
                description: Math.round(pane.controller.preference(
                    "appearance", "glass_edge_width", 24)) + " px"
                SettingsSlider {
                    from: 4
                    to: 128
                    stepSize: 1
                    value: pane.controller.preference("appearance", "glass_edge_width", 24)
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "glass_edge_width", Math.round(next))
                }
            }

            // Frosted is a flat surface, so there is no bend to set.
            SettingsRow {
                title: "Bend Strength"
                description: Math.round(pane.controller.preference(
                    "appearance", "glass_refraction", 0.7) * 100) + "%"
                visible: pane.glassMode === "liquid"
                SettingsSlider {
                    from: 0
                    to: 1
                    stepSize: 0.05
                    value: pane.controller.preference("appearance", "glass_refraction", 0.7)
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "glass_refraction", next)
                }
            }

            SettingsRow {
                title: "Edge Clarity"
                description: Math.round(pane.controller.preference(
                    "appearance", "glass_clarity", 0.75) * 100) + "% detail"
                SettingsSlider {
                    from: 0
                    to: 1
                    stepSize: 0.05
                    value: pane.controller.preference("appearance", "glass_clarity", 0.75)
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "glass_clarity", next)
                }
            }

            SettingsRow {
                title: "Rim Light"
                description: Math.round(pane.controller.preference(
                    "appearance", "glass_highlight", 0.12) * 100) + "%"
                SettingsSlider {
                    from: 0
                    to: 1
                    stepSize: 0.01
                    value: pane.controller.preference("appearance", "glass_highlight", 0.12)
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "glass_highlight", next)
                }
            }
        }

        SettingsGroup {
            title: "NIGHT SHIFT"

            SettingsRow {
                title: "Night Shift"
                description: "Warm display colors automatically during the configured schedule."
                SettingsSwitch {
                    checked: pane.controller.preference("appearance", "night_shift_enabled", true)
                    onToggled: value => pane.controller.setPreference("appearance", "night_shift_enabled", value)
                }
            }

            MenuSeparator { Layout.fillWidth: true }

            SettingsRow {
                title: "Starts At"
                description: "Times use the system's local clock."
                TimeField {
                    value: pane.controller.preference("appearance", "night_shift_start", "18:00")
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "night_shift_start", next)
                }
            }

            SettingsRow {
                title: "Ends At"
                TimeField {
                    value: pane.controller.preference("appearance", "night_shift_end", "06:00")
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "night_shift_end", next)
                }
            }

            SettingsRow {
                title: "Color Temperature"
                description: Math.round(pane.controller.preference(
                    "appearance", "night_shift_temperature", 4500)) + " K"
                SettingsSlider {
                    from: 1000
                    to: 10000
                    stepSize: 100
                    value: pane.controller.preference("appearance", "night_shift_temperature", 4500)
                    onCommitted: next => pane.controller.setPreference(
                        "appearance", "night_shift_temperature", Math.round(next))
                }
            }
        }

        Item { Layout.preferredHeight: 20 }
    }
}
