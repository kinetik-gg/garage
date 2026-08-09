-- Monitor wiki https://wiki.hypr.land/Configuring/Basics/Monitors/
--
-- One catch-all rule, and deliberately nothing else. This file ships with the
-- product, so it cannot know the machine it lands on: no connector name, no
-- position, no scale. An empty output is Hyprland's own `monitor = , preferred,
-- auto, 1` -- every display that connects comes up at its preferred mode, laid
-- out left to right in the order it is seen, unscaled. That is a usable desktop
-- on any hardware, which is the whole of what this rule is for.
--
-- The per-display rules are generated rather than written here. `garage render`
-- turns ~/.config/garage/displays.toml into
-- ~/.local/state/garage/generated/displays.lua, and hyprland.lua loads that
-- fragment after this file; a later rule naming an output replaces the earlier
-- one, so the generated rules win over the catch-all outright. The first login
-- has no displays.toml yet, so that first render writes one from the monitors
-- Hyprland reports -- the arrangement the rule below produced -- and the
-- machine's own topology is recorded from then on.
--
-- So change a display in the Displays pane, or in displays.toml, rather than
-- here. A rule added here would be a machine-specific fact in a tracked file,
-- and the generated fragment would override it anyway.
hl.monitor({
    output = "",
    mode = "preferred",
    position = "auto",
    scale = "1",
})
