-- Monitor wiki https://wiki.hypr.land/Configuring/Basics/Monitors/
-- Example: output can be found with hyprctl monitors. Edit variables.lua for the monitor outputs instead of here directly
-- hl.monitor({
--     output    = "MONITOR1",
--     mode      = "1920x1080@60",
--     position  = "0x0",
--     scale     = "1",
-- })

-- Safe fallback for connectors that change after a reinstall or GPU switch.
hl.monitor({
    output = "",
    mode = "preferred",
    position = "auto",
    scale = "1",
})

hl.monitor({
    output    = MONITOR3,
    mode      = "preferred",
    position  = "0x0",
    scale     = "1",
})

hl.monitor({
    output    = MONITOR1,
    mode      = "preferred",
    position  = "1920x0",
    -- 5/3 is the closest integer-logical-size match to the panel's 1.76 DPI ratio.
    scale     = "1.666667",
})

hl.monitor({
    output    = MONITOR2,
    mode      = "preferred",
    position  = "4224x0",
    scale     = "1",
})
