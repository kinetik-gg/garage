# shellcheck shell=bash
# INSTALL.md row 11: reload, mask, and enable the per-user systemd
# units without starting the graphical session.

# ---------------------------------------------------------------------------
# Per-user services
#
# `systemctl --user enable` is symlink bookkeeping and works from a TTY login,
# where a user manager exists but no graphical session does. Nothing is started
# here: every unit below is WantedBy=graphical-session.target (or timers.target)
# and comes up on the first graphical login.
# ---------------------------------------------------------------------------

step "Enabling the per-user services"

# The units were just linked into ~/.config/systemd/user, so the manager has to
# re-read them before enable can resolve their [Install] sections.
run systemctl --user daemon-reload

# The vanilla Arch Hyprland profile may pull in Dunst. This desktop uses the
# Garage shell as both the notification daemon and the control center, so stop
# D-Bus from activating Dunst first: it must never race the shell for the
# org.freedesktop.Notifications name. Not load-bearing: Dunst may not be
# installed at all.
run systemctl --user mask dunst.service || true

# Data, for the same reasons as packages.list: system/manifest/units.list. Field
# 2 is `running` or `oneshot` -- what a healthy session looks like for that unit,
# which is `garage doctor`'s question rather than this script's. It is read here
# only to reject a line that has no flag at all, because a unit whose kind nobody
# declared is a unit the doctor will silently stop checking.
user_units=()
while IFS= read -r manifest_line; do
    manifest_line=${manifest_line%%#*}
    read -r unit_name unit_kind _ <<<"$manifest_line"
    [[ -n $unit_name ]] || continue
    if [[ $unit_kind != running && $unit_kind != oneshot ]]; then
        echo "error: system/manifest/units.list: $unit_name is not marked" \
             "running or oneshot." >&2
        exit 1
    fi
    user_units+=("$unit_name")
done <"$repo_dir/system/manifest/units.list"

if ((${#user_units[@]} == 0)); then
    echo "error: system/manifest/units.list named no units." >&2
    exit 1
fi

run systemctl --user enable "${user_units[@]}"
record "enabled ${#user_units[@]} per-user units"

# Nothing renders or applies here. The first full render-and-apply happens at
# the first graphical login, as the `garage apply` in autostart.lua; the
# hyprpaper/hypridle units each render their own narrow fragment in an
# ExecStartPre on the way up (the bar's marker is written by garage apply).
# From a TTY there is no compositor for any of it to talk to.
