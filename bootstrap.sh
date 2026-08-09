#!/usr/bin/env bash
#
# Garage bootstrap.
#
# Target: a freshly installed, minimal Arch system with NO desktop environment.
# This runs from a bare TTY login -- nothing here may assume a Wayland session,
# a running compositor, or a graphical toolkit. Garage installs Hyprland itself;
# installing Arch stays the user's job.
#
# Usage: ./bootstrap.sh [--dry-run]
#        GARAGE_FORCE=1 ./bootstrap.sh   # skip the freshness gate

set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

dry_run=0
case "${1-}" in
    --dry-run) dry_run=1 ;;
    -h | --help)
        printf 'Usage: %s [--dry-run]\n' "${0##*/}"
        printf '\n  --dry-run   print every mutating action without executing it\n'
        printf '\nEnvironment:\n  GARAGE_FORCE=1   skip the freshness gate\n'
        exit 0
        ;;
    "") ;;
    *)
        echo "Unknown argument: $1 (only --dry-run is accepted)" >&2
        exit 2
        ;;
esac

# ---------------------------------------------------------------------------
# Output and execution helpers
# ---------------------------------------------------------------------------

step() { printf '\n==> %s\n' "$*"; }
info() { printf '    %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

# Every mutating command goes through run(). In --dry-run it is printed and not
# executed, which is what makes a dry run provably side-effect free: there is no
# second path that mutates anything.
run() {
    if ((dry_run)); then
        printf '    [dry-run] %s\n' "$*"
        return 0
    fi
    "$@"
}

# File bodies are written by heredoc/redirection, which run() cannot wrap.
write_file() {
    local path=$1
    if ((dry_run)); then
        printf '    [dry-run] write %s\n' "$path"
        cat >/dev/null
        return 0
    fi
    mkdir -p -- "$(dirname -- "$path")"
    cat >"$path"
}

summary=()
record() {
    summary+=("$1")
    info "$1"
}

# ---------------------------------------------------------------------------
# Environment preconditions
# ---------------------------------------------------------------------------

if [[ ${EUID} -eq 0 ]]; then
    fail "Run this as the desktop user, not root. It calls sudo where it needs to."
fi

if [[ ! -e /etc/arch-release ]] || ! command -v pacman >/dev/null; then
    fail "This bootstrap targets an Arch Linux installation (pacman and /etc/arch-release)."
fi

# ---------------------------------------------------------------------------
# Freshness gate
#
# Garage is a whole desktop, not a theme you drop onto an existing one. It
# claims ~/.config wholesale, enables its own display manager and its own
# session, and replaces the notification daemon. On a system that already has a
# desktop, all of that collides. Refusing loudly is the honest answer; adopting
# someone else's configuration is not something this installer can do safely.
# ---------------------------------------------------------------------------

# Where a symlink points after exactly one hop, as an absolute path, without
# resolving any further. Full resolution is wrong for this job: several tracked
# files are themselves symlinks (systemd .wants entries pointing at /usr/lib), so
# `readlink -m` on a perfectly good stow link lands outside the repository.
link_hop() {
    local target=$1 dest
    dest=$(readlink -- "$target")
    [[ $dest == /* ]] || dest="$(dirname -- "$target")/$dest"
    realpath -ms -- "$dest"
}

# True when a symlink is one of ours in *this* checkout. Checked lexically first,
# then through full resolution, so it holds whether or not the repository path
# itself contains symlinked components.
points_into_repo() {
    local target=$1
    [[ "$(link_hop "$target")" == "$repo_dir/desktop/"* ]] && return 0
    [[ "$(readlink -m -- "$target")" == "$repo_dir/desktop/"* ]] && return 0
    return 1
}

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
        freshness_findings+=("another desktop session is installed: $(echo "$foreign_sessions" | tr '\n' ' ')")
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

# ---------------------------------------------------------------------------
# Packages
# ---------------------------------------------------------------------------

packages=(
    base-devel git stow cmake meson cpio pkgconf linux-headers
    hyprland uwsm xdg-desktop-portal-hyprland xdg-desktop-portal-gtk
    hypridle hyprlock hyprpaper hyprpolkitagent hyprsunset
    waybar quickshell rofi kitty fish fisher
    swaync swayosd cliphist wl-clipboard playerctl
    grim slurp satty hyprpicker
    networkmanager bluez bluez-utils
    pipewire pipewire-alsa pipewire-pulse wireplumber pavucontrol
    nautilus gvfs gvfs-smb gnome-text-editor gnome-calculator loupe
    btop fastfetch micro qt6ct qt6-wayland xsettingsd
    python python-pip python-pipx uv lua jq zenity file libnotify 7zip
    papirus-icon-theme adw-gtk-theme
    noto-fonts noto-fonts-cjk noto-fonts-emoji ttf-ibm-plex
    ttf-cascadia-mono-nerd awesome-terminal-fonts
    remmina freerdp sddm lm_sensors xdg-user-dirs desktop-file-utils
    spotify-launcher discord zed
    docker docker-buildx docker-compose
    nodejs npm pnpm bun deno typescript-language-server bash-language-server
    rustup rust-analyzer
    clang lldb gdb ninja ccache mold valgrind perf strace
    shellcheck shfmt ripgrep fd fzf bat eza hyperfine just protobuf
    man-db man-pages ffmpeg imagemagick obs-studio
)

if command -v lspci >/dev/null && lspci | grep -qi nvidia; then
    packages+=(nvidia-open nvidia-utils egl-wayland libva-nvidia-driver)
fi

step "Refreshing the package database and upgrading the system"
# A full upgrade with no targets first, so the name check below reads a synced
# database and the install itself is never a partial upgrade.
run sudo pacman -Syu

step "Verifying every package name against the repositories"
# A renamed or dropped package used to abort the run halfway through. Report the
# whole list up front instead.
if pacman -Si pacman >/dev/null 2>&1; then
    missing=()
    for package in "${packages[@]}"; do
        pacman -Si "$package" >/dev/null 2>&1 || missing+=("$package")
    done
    if ((${#missing[@]})); then
        printf 'error: these packages are not in the configured repositories:\n' >&2
        for package in "${missing[@]}"; do
            printf '  - %s\n' "$package" >&2
        done
        printf '\nRun `sudo pacman -Syy`, check that the extra repository is enabled,\n' >&2
        printf 'then fix the list in bootstrap.sh before re-running.\n' >&2
        exit 1
    fi
    info "all ${#packages[@]} package names resolve."
else
    warn "the package database is not synced yet; skipping the name check."
fi

step "Installing the desktop package set"
run sudo pacman -S --needed "${packages[@]}"
record "installed ${#packages[@]} packages"

# ---------------------------------------------------------------------------
# System services and the login shell
# ---------------------------------------------------------------------------

step "Enabling system services"
# sddm is enabled here so the reboot at the end lands in a graphical login.
run sudo systemctl enable NetworkManager.service bluetooth.service sddm.service
run sudo systemctl enable --now docker.service
record "enabled NetworkManager, bluetooth, sddm and docker"

# Deliberately NOT adding the user to the `docker` group: membership is
# equivalent to passwordless root on this machine, and that is not a decision an
# installer should make for you. See docs/INSTALL.md if you want to opt in.

run sudo usermod -s /usr/bin/fish "$USER"
record "set fish as the login shell"

step "Creating the home directory layout"
# xdg-user-dirs-update is deliberately not called: ~/.config/user-dirs.dirs is a
# symlink into this repository, and the updater would rewrite a tracked file.
for directory in \
    "${HOME}/Desktop" \
    "${HOME}/Documents" \
    "${HOME}/Downloads" \
    "${HOME}/Music" \
    "${HOME}/Pictures" \
    "${HOME}/Projects" \
    "${HOME}/Public" \
    "${HOME}/repositories" \
    "${HOME}/Templates" \
    "${HOME}/Videos" \
    "${HOME}/.local/share/wallpaper"; do
    [[ -d $directory ]] || run mkdir -p -- "$directory"
done

# ---------------------------------------------------------------------------
# Link the tracked configuration into $HOME
#
# `stow` on its own aborts on the first conflict and leaves a half-linked home,
# and `stow -D` will not clean up absolute symlinks left behind by a checkout
# that has since moved. So: work out what stow is about to claim, move real
# files aside into a timestamped backup, delete stale links from other Garage
# checkouts by resolving them ourselves, and only then restow.
# ---------------------------------------------------------------------------

stow_ignore_patterns=()
while IFS= read -r pattern; do
    # Only the path-anchored entries matter here; the rest of the file repeats
    # stow's name-based defaults, which are handled below.
    [[ $pattern == '^/'* ]] && stow_ignore_patterns+=("$pattern")
done <"$repo_dir/desktop/.stow-local-ignore"

stow_ignores() {
    local rel="/$1" pattern
    for pattern in "${stow_ignore_patterns[@]}"; do
        [[ $rel =~ $pattern ]] && return 0
    done
    case ${1##*/} in
        .stow-local-ignore | .gitignore | *'~') return 0 ;;
    esac
    [[ $1 == .git/* || $1 == */.git/* ]] && return 0
    return 1
}

# Everything stow will place, as paths relative to $HOME. With --no-folding
# every leaf becomes its own symlink and every directory a real directory, so
# the conflict set is exactly the file list plus its ancestor directories.
managed_paths() {
    (cd "$repo_dir/desktop" && find . \( -type f -o -type l \) -printf '%P\n' | sort)
}

to_backup=()
to_unlink=()
declare -A ancestor_seen=()

# A symlink pointing at .../desktop/<the same relative path> is a stow link from
# a Garage checkout that has since moved. stow -D will not clean those up -- it
# only removes links it recognises as relative to the package it was given, and
# an old link is frequently absolute -- so resolve and delete them here instead
# of trusting stow with it.
foreign_garage_link() {
    local rel=$1
    [[ "$(link_hop "$HOME/$rel")" == */desktop/$rel ]] && return 0
    [[ ! -e "$HOME/$rel" ]] && return 0 # dangling: nothing of value to keep
    return 1
}

classify_ancestors() {
    local rel=$1 dir target
    dir=$(dirname -- "$rel")
    while [[ $dir != "." && $dir != "/" ]]; do
        if [[ -z ${ancestor_seen[$dir]-} ]]; then
            ancestor_seen[$dir]=1
            target="$HOME/$dir"
            if [[ -L $target ]]; then
                # A folded directory link -- from this checkout or another one --
                # blocks --no-folding from creating a real directory here.
                if points_into_repo "$target" || foreign_garage_link "$dir"; then
                    to_unlink+=("$dir")
                elif [[ ! -d $target ]]; then
                    to_backup+=("$dir")
                fi
            elif [[ -e $target && ! -d $target ]]; then
                to_backup+=("$dir")
            fi
        fi
        dir=$(dirname -- "$dir")
    done
}

step "Scanning \$HOME for anything in the way"
while IFS= read -r rel; do
    stow_ignores "$rel" && continue
    classify_ancestors "$rel"
    target="$HOME/$rel"
    if [[ -L $target ]]; then
        points_into_repo "$target" && continue # already ours
        if foreign_garage_link "$rel"; then
            to_unlink+=("$rel")
        else
            to_backup+=("$rel")
        fi
    elif [[ -e $target ]]; then
        to_backup+=("$rel")
    fi
done < <(managed_paths)

if ((${#to_unlink[@]})); then
    info "removing ${#to_unlink[@]} stale link(s) from a previous or moved checkout"
    for rel in "${to_unlink[@]}"; do
        info "  stale link: ~/$rel -> $(readlink -- "$HOME/$rel" 2>/dev/null || echo '?')"
        run rm -f -- "$HOME/$rel"
    done
    record "removed ${#to_unlink[@]} stale symlink(s)"
fi

if ((${#to_backup[@]})); then
    backup_root="$HOME/.garage-backup/$(date +%Y%m%d-%H%M%S)"
    info "moving ${#to_backup[@]} existing path(s) to $backup_root"
    for rel in "${to_backup[@]}"; do
        [[ -e "$HOME/$rel" || -L "$HOME/$rel" ]] || continue
        info "  backup: ~/$rel"
        run mkdir -p -- "$backup_root/$(dirname -- "$rel")"
        run mv -- "$HOME/$rel" "$backup_root/$rel"
    done
    record "backed up ${#to_backup[@]} pre-existing path(s) to $backup_root"
else
    info "nothing in the way."
fi

step "Linking the tracked configuration into \$HOME"
if ((dry_run)); then
    # --simulate is stow's own read-only mode. The per-link log is thousands of
    # lines, so count it and pass through only what is not a routine operation --
    # which is where a conflict report would appear.
    if ! stow_output=$(stow --dir="$repo_dir" --target="$HOME" --restow \
        --no-folding --simulate --verbose=1 desktop 2>&1); then
        printf '%s\n' "$stow_output" | sed 's/^/    /' >&2
        warn "stow --simulate reported a problem; see above."
    else
        operations=$(printf '%s\n' "$stow_output" | grep -c '^\(LINK\|UNLINK\|MKDIR\|RMDIR\):' || true)
        info "[dry-run] stow would perform $operations link operations."
        printf '%s\n' "$stow_output" | grep -v '^\(LINK\|UNLINK\|MKDIR\|RMDIR\):' |
            sed 's/^/    [dry-run] /' || true
    fi
else
    if ! stow_output=$(stow --dir="$repo_dir" --target="$HOME" --restow --no-folding desktop 2>&1); then
        printf '%s\n' "$stow_output" >&2
        cat >&2 <<'STOW'

stow could not link the configuration and this bootstrap has stopped rather
than leave your home half-installed. Each line above names a path that already
exists and is not a Garage link. Move those aside (or delete them) and re-run
./bootstrap.sh -- anything Garage itself knew about has already been moved to
~/.garage-backup/.

STOW
        exit 1
    fi
    [[ -n $stow_output ]] && printf '%s\n' "$stow_output"
fi
record "linked the tracked configuration with stow --no-folding"

# ---------------------------------------------------------------------------
# Per-user generated files
#
# These two carry an absolute $HOME inside them, so they cannot be tracked. They
# are written as real files directly into ~/.config, after stow, and only when
# absent -- your edits survive a re-run. Older bootstraps wrote them into the
# repository tree and relied on a stow link; such a link is replaced here.
# ---------------------------------------------------------------------------

step "Writing the per-user generated files"

needs_real_file() {
    local path=$1
    [[ -e $path || -L $path ]] || return 0
    if [[ -L $path ]] && points_into_repo "$path"; then
        run rm -f -- "$path"
        return 0
    fi
    return 1
}

swayosd_config="$HOME/.config/swayosd/config.toml"
if needs_real_file "$swayosd_config"; then
    sed "s|@HOME@|${HOME}|g" "$repo_dir/templates/swayosd-config.toml" |
        write_file "$swayosd_config"
    record "wrote ~/.config/swayosd/config.toml"
else
    info "keeping the existing ~/.config/swayosd/config.toml"
fi

gtk_bookmarks="$HOME/.config/gtk-3.0/bookmarks"
if needs_real_file "$gtk_bookmarks"; then
    write_file "$gtk_bookmarks" <<BOOKMARKS
file://${HOME}/Documents Documents
file://${HOME}/Downloads Downloads
file://${HOME}/Pictures Pictures
file://${HOME}/repositories Repositories
BOOKMARKS
    record "wrote ~/.config/gtk-3.0/bookmarks"
else
    info "keeping the existing ~/.config/gtk-3.0/bookmarks"
fi

if [[ ! -e "${HOME}/.local/share/wallpaper/current" ]]; then
    run ln -s "$repo_dir/desktop/Wallpaper/Dark/Nebula - Martin Martz.jpg" \
        "${HOME}/.local/share/wallpaper/current"
    record "selected the default wallpaper"
fi

# ---------------------------------------------------------------------------
# Per-user services
#
# `systemctl --user enable` is symlink bookkeeping and works from a TTY login,
# where a user manager exists but no graphical session does. Nothing is started
# here: every unit below is WantedBy=graphical-session.target (or timers.target)
# and comes up on the first graphical login.
# ---------------------------------------------------------------------------

step "Enabling the per-user services"
if ! systemctl --user show-environment >/dev/null 2>&1; then
    fail "No systemd user manager is reachable. Log in on a TTY as $USER (not via su) and re-run."
fi

# The units were just linked into ~/.config/systemd/user, so the manager has to
# re-read them before enable can resolve their [Install] sections.
run systemctl --user daemon-reload

# The vanilla Arch Hyprland profile may pull in Dunst. This desktop uses SwayNC
# as both the notification daemon and the control center, so stop D-Bus from
# activating Dunst first. Not load-bearing: Dunst may not be installed at all.
run systemctl --user mask dunst.service || true

user_units=(
    waybar.service    # + ExecStartPre=garage render-bar
    hyprpaper.service # + ExecStartPre=garage render-wallpaper
    hypridle.service  # + ExecStartPre=garage render  (the first full render)
    hyprsunset.service
    hyprpolkitagent.service
    swaync.service
    swayosd.service
    cliphist.service
    cliphist-image.service
    xsettingsd.service
    garage-shell.service
    garage-theme.timer
    garage-night-shift.timer
)
run systemctl --user enable "${user_units[@]}"
record "enabled ${#user_units[@]} per-user units"

# `garage render` is deliberately NOT called here. render_all() reloads Hyprland
# through `hyprctl`, which fails with no compositor running and makes the whole
# render error out -- there is nothing sensible for it to do from a TTY. The
# first full render happens at the first graphical login instead, as
# hypridle.service's ExecStartPre (see desktop/.config/systemd/user/
# hypridle.service.d/garage.conf), with waybar and hyprpaper rendering their own
# fragments the same way.

# ---------------------------------------------------------------------------
# Toolchains
# ---------------------------------------------------------------------------

step "Setting up the shell prompt and toolchains"

# Pinned: an unpinned prompt plugin is a third party with write access to every
# future shell start. `fisher install owner/repo@ref` checks out that ref.
pure_pin="pure-fish/pure@v4.18.0"
if fish -c 'type -q fisher'; then
    # fish_plugins is fisher's own manifest and is what it reconciles against.
    # `fisher list` is not used here: it reads a fish universal variable, which
    # is per-machine state that does not exist on a first run and can go missing
    # on a machine where the prompt is in fact installed.
    if grep -qF 'pure-fish/pure' "$HOME/.config/fish/fish_plugins" 2>/dev/null; then
        info "the Pure prompt is already installed."
    else
        run fish -c "fisher install $pure_pin"
        record "installed the Pure prompt ($pure_pin)"
    fi
fi

if command -v rustup >/dev/null; then
    run rustup default stable
    run rustup component add rustfmt clippy
    record "installed the stable Rust toolchain"
fi

# ---------------------------------------------------------------------------
# Optional Hyprland plugins
#
# Glass is a first-party repository the user develops in; the deploy script
# treats it as read-only and verifies it sits on the pinned commit. Garage's
# repositories are local-only today, so a missing checkout is the normal case
# and must not be fatal: hyprland.lua guards both plugin loads, so the desktop
# comes up without them.
# ---------------------------------------------------------------------------

step "Deploying the optional Hyprland plugins"
glass_repo=""
for candidate in "$HOME/repositories/glass" "$HOME/repositories/hyprliquid"; do
    [[ -d "$candidate/.git" ]] && {
        glass_repo=$candidate
        break
    }
done

if [[ -z $glass_repo ]]; then
    warn "no Glass plugin source at ~/repositories/glass -- skipping the plugin build."
    warn "  Garage's repositories are not published yet, so this is expected."
    warn "  The desktop runs without plugins; hyprland.lua treats both as optional."
    warn "  Once you have the source, run: ~/.config/hypr/scripts/garage-rebuild-plugins"
    summary+=("skipped the optional plugins (no Glass source)")
elif ((dry_run)); then
    info "[dry-run] $HOME/.config/hypr/scripts/garage-rebuild-plugins (source: $glass_repo)"
elif ! "$HOME/.config/hypr/scripts/garage-rebuild-plugins"; then
    warn "the plugin build failed; Hyprland will still start without the optional plugins."
    summary+=("optional plugin build FAILED (non-fatal)")
else
    record "deployed the pinned Hyprland plugins from $glass_repo"
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

printf '\n'
if ((dry_run)); then
    printf 'Dry run complete. Nothing above was executed.\n\n'
    exit 0
fi

printf 'Garage is installed.\n\n'
for line in "${summary[@]}"; do
    printf '  - %s\n' "$line"
done
cat <<'DONE'

Reboot to enter your desktop:

    sudo reboot

SDDM will start on the next boot; pick "Hyprland (UWSM)" and log in. The first
login renders your settings, wallpaper and bar, so it takes a few seconds
longer than the ones after it.

DONE
