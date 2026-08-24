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
    -- Keep the legacy garage-launcher selector until every running Quickshell
    -- instance has reloaded the two-surface launcher. The new interactive host
    -- is garage-launcher-host and deliberately absent from this rule. The bar
    -- tints itself over this blur the way the old stylesheet did.
    match = { namespace = "^(garage-bar|notifications|rofi|garage-notifications|garage-launcher|garage-launcher-glass|garage-screenshot|garage-session-menu|garage-notification-center|garage-control-center|garage-monitor|garage-media|garage-ai-usage|garage-extension-[a-z0-9-]+|garage-osd)$" },
    blur = true,
    blur_popups = true,
    ignore_alpha = 0.15,
})
-- Every panel the shell opens animates itself, and moves its own layer surface
-- to do it: PanelMotion.qml drives margins.top, so the compositor repositions
-- the whole surface -- Glass, frame and contents together -- on every frame.
--
-- They are all top-to-bottom now. The two centres and the AI usage panel used to
-- slide in from the right edge they are anchored to, which said the panel had
-- arrived from off-screen rather than from the control that opened it; the ones
-- not against a side could not use that rule at all, so the shell had three
-- different entrances for one kind of surface.
--
-- no_anim rather than no rule at all, so a compositor animation cannot move the
-- same surface underneath the client's own.
hl.layer_rule({
    name = "shell-palette-client-animation",
    match = { namespace = "^(garage-notification-center|garage-control-center"
        .. "|garage-ai-usage|garage-extension-[a-z0-9-]+"
        .. "|garage-media|garage-monitor)$" },
    no_anim = true,
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
    -- The bar draws and re-lays itself out in place; a compositor animation
    -- would move a surface that never asked to travel. The launcher host and
    -- its glass backing both animate through PanelMotion, and neither may
    -- receive a second animation from the compositor.
    match = { namespace = "^(garage-bar|rofi|garage-launcher|garage-launcher-host|garage-launcher-glass|garage-session-menu|garage-session-confirmation|garage-osd)$" },
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
