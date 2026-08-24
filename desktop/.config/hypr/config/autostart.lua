-- Auto-start config
-- if you dont use UWSM add your auto start programs here, otherwise use XDG autostart https://wiki.archlinux.org/title/XDG_Autostart

hl.on("hyprland.start", function ()
    hl.exec_cmd("dbus-update-activation-environment --systemd --all")
    hl.exec_cmd("~/.local/bin/garage reconcile")
    hl.exec_cmd("~/.local/bin/garage apply")
    -- Keep session infrastructure supervised by systemd. The bar lives in
    -- garage-shell now; there is no separate bar unit to start.
    hl.exec_cmd("systemctl --user start garage-shell.service")
    hl.exec_cmd("systemctl --user start hypridle.service")
    hl.exec_cmd("systemctl --user start hyprsunset.service garage-night-shift.timer")
    hl.exec_cmd("systemctl --user start cliphist.service")
    hl.exec_cmd("systemctl --user start cliphist-image.service")
    hl.exec_cmd("systemctl --user start hyprpolkitagent.service")
end)
