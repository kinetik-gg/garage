-- Keep the desktop restrained: only Spaces-style workspace movement animates.
hl.config({
    animations = {
        enabled = true,
        workspace_wraparound = false,
    },
})

-- Fast macOS-like ease-out: decisive movement with a soft landing and no bounce.
hl.curve("workspaceEase", {
    type = "bezier",
    points = { { 0.22, 1.0 }, { 0.36, 1.0 } },
})

-- The three enabled leaves below are the baseline the Motion controls scale.
-- appearance.reduce_motion overrides animations.enabled above and
-- appearance.animation_speed divides every speed here, both in the generated
-- fragment Hyprland loads after this file. Their durations are mirrored in
-- ANIMATION_LEAVES in garage; what is written here is the
-- multiplier-of-1 baseline and the fallback when no fragment exists.
hl.animation({ leaf = "global", enabled = true, speed = 3.0, bezier = "workspaceEase" })

-- Preserve the otherwise instant desktop behavior.
hl.animation({ leaf = "windows", enabled = false })
hl.animation({ leaf = "layers", enabled = true, speed = 2.4, bezier = "workspaceEase" })
hl.animation({ leaf = "fade", enabled = false })
hl.animation({ leaf = "border", enabled = false })
hl.animation({ leaf = "borderangle", enabled = false })
hl.animation({ leaf = "zoomFactor", enabled = false })

-- Slide the full workspace horizontally; trackpad gestures remain 1:1.
hl.animation({
    leaf = "workspaces",
    enabled = true,
    speed = 3.0,
    bezier = "workspaceEase",
    style = "slide",
})
hl.animation({ leaf = "specialWorkspace", enabled = false })
