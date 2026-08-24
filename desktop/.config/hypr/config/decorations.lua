-- Minimal, neutral window treatment.
--
-- Values that depend on the light/dark theme -- group colours, shadow, dim
-- strength, and the glass tint and frame rings -- are overridden by
-- garage in generated/preferences.lua, which Hyprland loads after
-- this file. What is set here is the dark-theme baseline and the fallback when
-- that generated file is absent.

-- Corner geometry. Windows, the plugin's layer surfaces and the overview tiles
-- are three separate code paths that each need the numbers handed to them, so
-- they share one pair here rather than repeating literals that can drift.
-- appearance.corner_radius overrides ROUNDING in the generated fragment; the
-- power is fixed so only the size of a corner changes, never its profile.
-- Matches CORNER_RADII["normal"] in garage, which is the default.
local ROUNDING = 7
local ROUNDING_POWER = 3.37

hl.config({
    general = {
        gaps_in = 3,
        -- Inner gaps meet between two windows, so 3 + 3 renders as 6 px.
        gaps_out = 6,
        -- appearance.border_size overrides this in the generated fragment, and
        -- brings the colours below with it: a size over a transparent border
        -- would only shrink the window and draw nothing.
        border_size = 0,
        extend_border_grab_area = 10,
        resize_on_border = true,
        col = {
            active_border = "rgba(00000000)",
            inactive_border = "rgba(00000000)",
        },
    },
    group = {
        col = {
            border_active = "rgba(666666ff)",
            border_inactive = "rgba(181818ff)",
            border_locked_active = "rgba(888888ff)",
            border_locked_inactive = "rgba(181818ff)",
        },
        groupbar = {
            col = {
                active = "rgba(666666ff)",
                inactive = "rgba(181818ff)",
                locked_active = "rgba(888888ff)",
                locked_inactive = "rgba(181818ff)",
            },
        },
    },
    decoration = {
        dim_special = 0.1,
        dim_inactive = true,
        dim_strength = 0.25,
        rounding = ROUNDING,
        rounding_power = ROUNDING_POWER,
        active_opacity = 0.96,
        inactive_opacity = 0.96,
        fullscreen_opacity = 1.0,
        shadow = {
            enabled = true,
            range = 18,
            render_power = 3,
            sharp = false,
            color = "rgba(00000066)",
            color_inactive = "rgba(00000040)",
            scale = 0.98,
        },
        -- appearance.glass_mode = off turns this off in the generated fragment,
        -- along with the opacities above: the plugin is only one of the two
        -- things making a window see-through, and solid has to mean both.
        blur = {
            enabled = true,
            -- The Blur control's Normal step; it drives these together with the
            -- plugin's own blur_passes/blur_downscale below.
            size = 6,
            passes = 2,
            popups = true,
            popups_ignorealpha = 0.2,
        },
    },
})

if GLASS_AVAILABLE then
    hl.config({
        plugin = {
        kinetik_glass = {
            enabled = true,
            tint = "rgba(1c1c1eff)",
            tint_opacity = 0.02,
            blur_opacity = 1.0,
            -- How much of the backdrop's colour survives in the material;
            -- 1.0 leaves it untouched.
            saturation = 1.0,
            -- The surface is modelled as a thick slab with a rounded bevel.
            -- The index of refraction is fixed at 1.52 (window glass) in the
            -- shader because the bend barely responds to it; refraction is the
            -- bevel steepness, and refraction_power shapes its profile.
            refraction = 0.7,
            refraction_power = 1.0,
            dispersion = 0.7,
            -- How much detail survives in the refracting rim. The body stays
            -- frosted; without this the bend displaces an already-flat blur and
            -- is invisible.
            edge_clarity = 0.75,
            -- The plugin's own defaults, spelled out because the Blur control
            -- now moves them: this pair is its Normal step.
            blur_passes = 4,
            blur_downscale = 0.25,
            noise = 0.02,
            -- Bevel width in logical px: the decay length of the rounded edge.
            -- Sizing it near a visible corner radius keeps the refraction a rim
            -- rather than a body-wide warp; 24 came from the largest setting
            -- (14 * 3.37/2). That sizing is how this default was picked, not a
            -- rule: Edge Thickness is its own slider, so a changed corner radius
            -- deliberately leaves it alone.
            edge_width = 24,
            frame_highlight_opacity = 0.07,
            frame_border_opacity = 0.30,
            -- Windows now take their corner shape from decoration.rounding_power
            -- above, so this only applies to the layer surfaces listed below.
            corner_power = ROUNDING_POWER,
            -- Additive rim light living only on the bevel.
            highlight_opacity = 0.12,
            -- garage-launcher is the one-surface name used by a Quickshell
            -- process that has not reloaded yet. The replacement host is named
            -- garage-launcher-host and is intentionally absent; its shorter
            -- garage-launcher-glass backing owns the material after reload.
            layer_namespaces = "garage-screenshot=pill,garage-launcher,garage-launcher-glass,garage-session-menu,garage-session-confirmation,garage-notification-center,garage-control-center,garage-monitor,garage-media,garage-ai-usage,garage-osd=pill",
            layer_rounding = ROUNDING,
        },
    }})
end

if HYPREXPO_AVAILABLE then
    hl.config({
        plugin = {
        hyprexpo = {
            dynamic_grid = 1,
            fill_gaps = 1,
            mru_sort = 0,
            bg_col = "rgba(1c1c1ef2)",
            gaps_in = 14,
            gaps_out = 28,
            wallpaper_bg = 1,
            tile_rounding = ROUNDING,
            tile_rounding_power = ROUNDING_POWER,
            border_width = 1,
            border_color = "rgba(ffffff18)",
            border_color_current = "rgba(ffffffcc)",
            border_color_focus = "rgba(ffffffff)",
            border_color_hover = "rgba(ffffff80)",
            label_enable = 0,
            show_workspace_numbers = 0,
            show_cursor = 1,
            show_pinned_windows = 0,
            drag_drop_enable = 0,
            keynav_enable = 1,
            gesture_fingers = 4,
            gesture_direction = "up",
            gesture_distance = 240,
        },
    }})
end
