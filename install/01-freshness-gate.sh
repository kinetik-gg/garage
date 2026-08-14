# shellcheck shell=bash
# INSTALL.md row 1: detect an existing desktop and enforce the
# fresh-system installation policy.

# ---------------------------------------------------------------------------
# Freshness gate
#
# Garage is a whole desktop, not a theme you drop onto an existing one. It
# claims ~/.config wholesale, enables its own display manager and its own
# session, and replaces the notification daemon. On a system that already has a
# desktop, all of that collides. Refusing loudly is the honest answer; adopting
# someone else's configuration is not something this installer can do safely.
# ---------------------------------------------------------------------------

# A prior Garage install is not a "used system" -- re-running the bootstrap is a
# supported operation. Detected from a stowed file resolving back into this
# checkout rather than from a marker file, so it stays true after a move.
garage_already_installed() {
    local probe="$HOME/.config/hypr/hyprland.lua"
    [[ -L $probe ]] || return 1
    points_into_repo "$probe"
}

# Session files shipped by other desktops. hyprland.desktop and
# hyprland-uwsm.desktop are pacman's own (hyprland, uwsm) and are expected here.
foreign_session_files() {
    local session name
    for session in /usr/share/wayland-sessions/*.desktop /usr/share/xsessions/*.desktop; do
        [[ -e $session ]] || continue
        name=${session##*/}
        case $name in
            gnome* | plasma* | kde* | xfce* | cinnamon* | mate* | budgie* | lxqt* | lxde* | deepin* | pantheon* | cosmic* | sway* | i3* | openbox*)
                printf '%s\n' "$session"
                ;;
        esac
    done
}

# /etc/skel is what a freshly created Arch user starts with, so anything in
# ~/.config that skel does not account for is somebody's configuration.
count_config_entries() {
    local entry name total=0
    [[ -d "$HOME/.config" ]] || {
        printf '0\n'
        return 0
    }
    for entry in "$HOME"/.config/* "$HOME"/.config/.[!.]*; do
        [[ -e $entry || -L $entry ]] || continue
        name=${entry##*/}
        [[ -e "/etc/skel/.config/$name" ]] && continue
        total=$((total + 1))
    done
    printf '%s\n' "$total"
}

freshness_findings=()

if garage_already_installed; then
    info "Garage is already installed in this home; treating this as a re-run."
else
    if systemctl is-enabled display-manager.service >/dev/null 2>&1; then
        freshness_findings+=("display-manager.service is already enabled -- this system already has a login manager.")
    fi

    foreign_sessions="$(foreign_session_files)"
    if [[ -n $foreign_sessions ]]; then
        freshness_findings+=("another desktop session is installed: ${foreign_sessions//$'\n'/ }")
    fi

    config_entries="$(count_config_entries)"
    if ((config_entries > 3)); then
        freshness_findings+=("the ~/.config directory already holds $config_entries entries beyond /etc/skel -- Garage would have to displace them.")
    fi

    for helper in yay paru; do
        if command -v "$helper" >/dev/null; then
            freshness_findings+=("$helper is on PATH -- this system has been customised past a base install.")
        fi
    done
fi

if ((${#freshness_findings[@]})); then
    if [[ ${GARAGE_FORCE:-0} == 1 ]]; then
        warn "GARAGE_FORCE=1: continuing on a system that does not look fresh."
        for finding in "${freshness_findings[@]}"; do
            warn "  $finding"
        done
    elif ((dry_run)); then
        printf '\n'
        warn "the freshness gate WOULD REFUSE this system:"
        for finding in "${freshness_findings[@]}"; do
            warn "  $finding"
        done
        warn "continuing the dry run anyway, because a dry run changes nothing."
    else
        cat >&2 <<GATE

Garage expects a fresh system and this one is already in use.

GATE
        for finding in "${freshness_findings[@]}"; do
            printf '  - %s\n' "$finding" >&2
        done
        cat >&2 <<'GATE'

Garage is a complete desktop, not a theme. It takes over ~/.config, installs
its own display manager and session, and replaces the notification daemon. On a
system that already has a desktop those choices collide, and this installer
cannot adopt another desktop's configuration without losing it.

What to do instead:

  - Install Garage on a freshly installed, minimal Arch system with no desktop
    environment. That is the only configuration it is tested against.
  - Or, if you understand the consequences and want to proceed anyway:

        GARAGE_FORCE=1 ./bootstrap.sh

    Existing files at the paths Garage manages are moved to
    ~/.garage-backup/<timestamp>/ rather than deleted, but enabled services and
    installed packages are not reverted for you.
  - Use ./bootstrap.sh --dry-run to see every change it would make.

GATE
        exit 1
    fi
fi
