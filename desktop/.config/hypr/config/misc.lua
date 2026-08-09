hl.config({
    dwindle = {
        preserve_split = true,
    },
    misc = {
        background_color = "rgb(000000)",
        -- Whatever hyprpaper is not covering shows Hyprland's own wallpaper,
        -- picked at random from three built-ins, plus a version splash. That is
        -- what flashed between wallpaper changes; with these off the gap is the
        -- background_color above.
        disable_hyprland_logo = true,
        disable_splash_rendering = true,
        col = {
            splash = "rgb(000000)",
        },
        middle_click_paste = false,
        enable_swallow = true,
        swallow_regex = "(kitty|ghostty|[Kk]onsole|Alacritty|gnome-terminal|xfce[0-9]?-terminal)",
        vrr = 3,
    },
    xwayland = {
        force_zero_scaling = true
    },
    ecosystem = {
        no_update_news = true,
        no_donation_nag = true,
    },
})
