-- Window rules wiki https://wiki.hypr.land/Configuring/Basics/Window-Rules/

-- Generic floating position
hl.window_rule({ match = { float = true }, center = true, persistent_size = true })

-- Shell surfaces that are real toplevels rather than layer shells, so they can
-- be moved and stacked like any other window.
hl.window_rule({ match = { class = "^(org\\.quickshell)$" }, float = true, center = true })

-- Picture-in-Picture
hl.window_rule({
    match             = { title = "^([Pp]icture[-\\s]?[Ii]n[-\\s]?[Pp]icture)(.*)$" },
    float             = true,
    keep_aspect_ratio = true,
    size              = { "max(monitor_w, monitor_h)*0.25", "min(monitor_w, monitor_h)*0.25" },
    pin               = true,
})

-- Gaming
local gamingApps = "^(steam_app.*|gamescope)$"
local gamingWorkspace = "name:gaming"

hl.window_rule({ match = { content = "game" }, workspace = gamingWorkspace })
hl.window_rule({ match = { xdg_tag = "^(.*game.*)$" }, workspace = gamingWorkspace, fullscreen_state = 2, content = "game", sync_fullscreen = true })
hl.window_rule({ match = { class = gamingApps }, workspace = gamingWorkspace })
hl.window_rule({ match = { class = "^(steam)$", title = "^(Friends List)$" }, float = true })
hl.window_rule({ match = { class = "^(steam)$", title = "^(Launching\\.{3})$" }, float = true, center = true, workspace = gamingWorkspace })
hl.window_rule({
    match = {
        class         = gamingApps,
        title         = "^(.+)$",
        initial_title = "negative:^(.*\\\\home\\\\.*)$",
    },
    content          = "game",
    decorate         = false,
    fullscreen_state = 2,
    size             = { "monitor_w", "monitor_h" },
    sync_fullscreen  = true,
})
hl.window_rule({
    match = {
        class         = "^(steam_app.*)$",
        initial_title = "^$",
    },
    center           = true,
    float            = true,
    fullscreen       = false,
    fullscreen_state = 0,
    workspace        = gamingWorkspace,
})

-- Apps
hl.window_rule({ match = { class = "^([Rr]ofi)$" }, float = true, center = true, opacity = "1.0 override" })
hl.window_rule({
    name   = "nautilus-quick-look",
    match  = { class = "^(org\\.gnome\\.NautilusPreviewer)$" },
    float  = true,
    center = true,
    size   = { "monitor_w*0.62", "monitor_h*0.72" },
    no_anim         = true,
    persistent_size = false,
})

-- Translucent shell surfaces use compositor blur behind their own alpha.
hl.layer_rule({
    name = "apple-dark-shell-blur",
    match = { namespace = "^(waybar|notifications|rofi|garage-notifications|garage-launcher|garage-screenshot|garage-session-menu|garage-notification-center|garage-control-center|garage-monitor|garage-media|garage-ai-usage)$" },
    blur = true,
    blur_popups = true,
    ignore_alpha = 0.15,
})
-- The panels that come in from the right edge, which is every panel anchored to
-- it: the two centres and the two the bar opens beside them. garage-media is
-- deliberately not here -- it is anchored to the top edge alone and centred
-- across the output, so sliding it in from the right would carry it sideways
-- across the screen. It is left to the compositor's default the same way
-- garage-screenshot, the other surface that is not against a side, already is.
hl.layer_rule({
    name = "control-center-slide",
    match = { namespace = "^(garage-notification-center|garage-control-center|garage-monitor|garage-ai-usage)$" },
    animation = "slide right",
})
-- Media is centred rather than attached to an edge. Fade the layer itself so
-- Glass, the frame and the QML contents enter and leave as one surface; a QML
-- opacity animation only faded the client content and doubled the map animation.
hl.layer_rule({
    name = "media-palette-fade",
    match = { namespace = "^(garage-media)$" },
    animation = "fade",
})
-- Toast popups only: they appear involuntarily and can carry message content,
-- so they stay out of screen shares. The full panels -- the notification and
-- control centers, and the monitor, media and AI usage panels the bar opens --
-- are opened deliberately, and the owner explicitly wants to be able to
-- screenshot them, so they are all left out of this rule.
hl.layer_rule({
    name = "notification-screen-share-privacy",
    match = { namespace = "^(garage-notifications)$" },
    no_screen_share = true,
})
hl.layer_rule({
    name = "static-shell-layers",
    -- garage-launcher takes rofi's slot here: it replaced rofi, and a launcher
    -- opened from a keystroke should be under the pointer already rather than
    -- sliding in.
    match = { namespace = "^(waybar|rofi|garage-launcher|garage-session-menu|garage-session-confirmation)$" },
    no_anim = true,
})
hl.window_rule({ match = { class = "^([Bb]lender)$" }, opacity = "1.0 override" })
hl.window_rule({ match = { class = "^(google-chrome)$" }, opacity = "1.0 override" })
hl.window_rule({ match = { class = "^(.*\\.exe)$", float = true }, monitor = PRIMARY_MONITOR, center = true, fullscreen_state = 0 })
hl.window_rule({ match = { class = "^(.*[Ll]auncher.*)$" }, float = true, monitor = PRIMARY_MONITOR })
hl.window_rule({ match = { class = "^(vesktop|discord)$" }, monitor = PRIMARY_MONITOR })
hl.window_rule({ match = { class = "^(.*[Cc]alc.*)$" }, float = true, size = { "max(monitor_w, monitor_h)*0.17", "min(monitor_w, monitor_h)*0.43" } })
hl.window_rule({ match = { class = "^(org\\.kde\\.keditfiletype)$" }, float = true })
hl.window_rule({ match = { class = "^(org\\.kde\\.ark)$" }, size = { "max(monitor_w, monitor_h)*0.40", "min(monitor_w, monitor_h)*0.40" } })
hl.window_rule({ match = { class = "^(.*satty.*)$", title = "^(Satty)$" }, min_size = { "max(monitor_w, monitor_h)*0.35", "min(monitor_w, monitor_h)*0.35" }, float = true })

-- Float Utility Windows
local floatApps = {
    { class = "^(kvantummanager|qt[56]ct|nwg-look)$" },
    { class = "^(org.pulseaudio.pavucontrol|blueman-manager|nm-applet|nm-connection-editor)$" },
    { title = "^(Winetricks.*|Protontricks.*)$" },
}
for _, m in ipairs(floatApps) do hl.window_rule({ match = m, float = true }) end

-- Float Common Modals
local modalMatches = {
    { title = "^(Open|Authentication Required|Add Folder to Workspace|Choose Files|Save As|Confirm to replace files|File Operation Progress)$" },
    { initial_title = "^(Open File)$" },
    { class = "^([Xx]dg-desktop-portal-gtk)$" },
    { title = "^(File Upload|Choose wallpaper|Library)(.*)$" },
    { class = "^(.*dialog.*)$" },
    { title = "^(.*dialog.*)$" },
    { class = "^(hyprland-share-picker)$"},
}
for _, m in ipairs(modalMatches) do hl.window_rule({ match = m, float = true }) end

-- Ignore maximize requests from all apps. You'll probably like this.
hl.window_rule({
    name  = "suppress-maximize-events",
    match = { class = ".*" },
    suppress_event = "maximize",
})

-- Fix some dragging issues with XWayland
hl.window_rule({
    name  = "fix-xwayland-drags",
    match = {
        class      = "^$",
        title      = "^$",
        xwayland   = true,
        float      = true,
        fullscreen = false,
        pin        = false,
    },
    no_focus = true,
})
