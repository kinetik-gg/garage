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
        printf '\nEnvironment:\n  GARAGE_FORCE=1                 skip the freshness gate\n'
        printf '  GARAGE_SKIP_PLUGIN_DEPLOY=1    do not deploy the Hyprland plugins\n'
        printf '                                 (set by `garage update`, which decides that itself)\n'
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

# $USER is PAM's business and is unset in some contexts (docker exec, runuser);
# the kernel always knows who we are.
desktop_user="$(id -un)"

# Checked before anything mutates: the per-user enable step near the end needs a
# systemd user manager, and discovering that only there would strand a real run
# with the shell changed and the links already made. A dry run reports and
# continues -- it changes nothing, so it can afford to preview the full plan.
if ! systemctl --user show-environment >/dev/null 2>&1; then
    if ((dry_run)); then
        warn "no systemd user manager is reachable here; a real run would stop at this point."
    else
        fail "No systemd user manager is reachable. Log in on a TTY as $desktop_user (not via su) and re-run."
    fi
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

# ---------------------------------------------------------------------------
# Packages
# ---------------------------------------------------------------------------

packages=(
    base-devel git stow cmake meson cpio pkgconf linux-headers
    hyprland uwsm xdg-desktop-portal-hyprland xdg-desktop-portal-gtk
    hypridle hyprlock hyprpaper hyprpolkitagent hyprsunset
    waybar quickshell rofi kitty fish fisher
    swayosd cliphist wl-clipboard playerctl
    grim slurp satty hyprpicker
    networkmanager bluez bluez-utils
    pipewire pipewire-alsa pipewire-pulse wireplumber pavucontrol cava
    nautilus gvfs gvfs-smb gnome-text-editor gnome-calculator loupe
    btop fastfetch micro qt6ct qt6-wayland xsettingsd
    python python-pip python-pipx uv lua jq zenity file libnotify 7zip
    papirus-icon-theme adw-gtk-theme
    # UI faces (Plus Jakarta Sans, Geist Mono) are bundled under
    # desktop/.local/share/fonts, alongside Phosphor -- not Arch-packaged, and
    # picked up by the post-stow fc-cache run below.
    noto-fonts noto-fonts-cjk noto-fonts-emoji
    ttf-cascadia-mono-nerd awesome-terminal-fonts
    remmina freerdp sddm lm_sensors xdg-user-dirs desktop-file-utils pciutils
    spotify-launcher discord zed
    docker docker-buildx docker-compose
    nodejs npm pnpm bun deno typescript-language-server bash-language-server
    rustup rust-analyzer
    clang lldb gdb ninja ccache mold valgrind perf strace
    shellcheck shfmt ripgrep fd fzf bat eza hyperfine just protobuf
    man-db man-pages ffmpeg imagemagick obs-studio
)

# pciutils is in the `base` group, so lspci is there on a minimal install and
# this check works before the package phase. It is named in the list above
# anyway: two decisions now depend on it -- the NVIDIA driver set here and the
# window material gate further down -- and neither should quietly fall back to
# "cannot tell" because something removed it.
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

run sudo usermod -s /usr/bin/fish "$desktop_user"
record "set fish as the login shell"

step "Creating the home directory layout"
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

# user-dirs.dirs is machine-local mutable state, not tracked config:
# xdg-user-dirs-update.service rewrites it at every login, which would sever a
# stow link and leave a conflicting real file behind. Written once when absent,
# like the GTK bookmarks; the login-time updater owns it from then on.
if [[ ! -e "${HOME}/.config/user-dirs.dirs" ]]; then
    write_file "${HOME}/.config/user-dirs.dirs" <<'USERDIRS'
XDG_DESKTOP_DIR="$HOME/Desktop"
XDG_DOCUMENTS_DIR="$HOME/Documents"
XDG_DOWNLOAD_DIR="$HOME/Downloads"
XDG_MUSIC_DIR="$HOME/Music"
XDG_PICTURES_DIR="$HOME/Pictures"
XDG_PUBLICSHARE_DIR="$HOME/Public"
XDG_TEMPLATES_DIR="$HOME/Templates"
XDG_VIDEOS_DIR="$HOME/Videos"
USERDIRS
    record "wrote ~/.config/user-dirs.dirs"
fi

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
        if ((${#to_backup[@]} + ${#to_unlink[@]})); then
            # The simulate runs against the unmodified $HOME: the backups and
            # stale-link removals above have not actually happened in a dry run,
            # so stow still sees those paths in the way.
            info "[dry-run] conflicts at path(s) listed in the backup plan above are expected: a real run moves them before stow."
        else
            warn "stow --simulate reported a problem; see above."
        fi
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

# Refreshes the fontconfig cache so the bundled fonts -- Phosphor, Plus
# Jakarta Sans, and Geist Mono, just linked into ~/.local/share/fonts by
# stow -- are found at first login.
run fc-cache

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
# The window material gate
#
# Garage ships glass_mode = "liquid", which is the right default for the machine
# it was developed on and the wrong one for a laptop. Kinetik Glass in any
# enabled mode captures the entire monitor framebuffer, downscales it and runs a
# multi-pass blur over it *per glass window per damaged frame* -- see
# Glass::Blur::refresh(), which blurs `CBox whole{{}, bufferSize}` and is gated
# only on the plugin being enabled at all. On a discrete GPU that is free. On
# integrated graphics, sharing system memory bandwidth with the CPU, it is a
# frame-rate collapse, and a first login that stutters reads as a broken
# desktop rather than as a setting.
#
# So the default is decided here, from the hardware, before anything renders.
# Nothing is written on a machine with a discrete GPU: the shipped default is
# already right there, and a file that says the same thing as layer 1 is only a
# copy that can drift.
#
# Placed after the stow step rather than next to the package phase because it
# writes into ~/.config/garage, which the shipped defaults reach as a stow
# symlink, and it must land before the first render -- which happens at the
# first graphical login, not here.
# ---------------------------------------------------------------------------

step "Choosing a window material this GPU can afford"

# Every GPU-class PCI device, in lspci's -nn form so the classifier can read
# both the vendor id and the marketing name. Matched on the class code
# (0300 VGA, 0302 3D, 0380 Display) rather than on lspci's English description,
# which is a translation away from changing.
gpu_devices() {
    command -v lspci >/dev/null || return 0
    lspci -nn 2>/dev/null | grep -E '\[03(00|02|80)\]' || true
}

# discrete | integrated | unknown, from the lines gpu_devices produced.
#
# "integrated" is the acting verdict, so it has to be reached from positive
# evidence: devices were found and none of them is a discrete GPU. An empty list
# is "unknown" and changes nothing -- and so is an lspci that is not installed.
gpu_verdict() {
    local list=$1 line discrete=0 found=0
    while IFS= read -r line; do
        [[ -n $line ]] || continue
        found=1
        case $line in
            *'[10de:'*)
                # NVIDIA has never shipped an x86 integrated GPU.
                discrete=1
                ;;
            *'[8086:'*)
                # Intel is integrated apart from its discrete Arc line, which
                # names itself in the device string.
                if [[ $line =~ (Arc|DG1|DG2|Alchemist|Battlemage) ]]; then
                    discrete=1
                fi
                ;;
            *'[1002:'* | *'[1022:'*)
                # AMD prints an APU as a *CPU* codename plus a bare
                # "[Radeon Graphics]" or "[Radeon Vega ... Graphics]" -- e.g.
                # "Granite Ridge [Radeon Graphics]" -- and a card as a GPU family
                # plus a model number: "Navi 31 [Radeon RX 7900 XT ...]". Keying
                # on the model number and the discrete family names means the
                # next APU codename is classified correctly without this file
                # being edited, which is the opposite of what an APU codename
                # list would do.
                if [[ $line =~ Radeon\ (RX|Pro|R[579])[[:space:]]*[0-9] ]]; then
                    discrete=1
                fi
                if [[ $line =~ (Navi|Ellesmere|Baffin|Polaris|Hawaii|Tonga|Fiji|Lexa|Oland|Bonaire|Pitcairn|Tahiti|Curacao) ]]; then
                    discrete=1
                fi
                ;;
            *)
                # An unrecognised vendor at a GPU class code is a virtual
                # adapter (virtio, QXL, VMware SVGA) or a server BMC (ASPEED,
                # Matrox). None of those can afford the effect either, and they
                # are exactly the machines a full-framebuffer blur punishes
                # hardest, so they count as evidence for the light default
                # rather than as no evidence at all.
                ;;
        esac
    done <<<"$list"
    if ((discrete)); then
        printf 'discrete\n'
    elif ((found)); then
        printf 'integrated\n'
    else
        printf 'unknown\n'
    fi
}

gpu_lines="$(gpu_devices)"
gpu_class="$(gpu_verdict "$gpu_lines")"
if [[ -n $gpu_lines ]]; then
    while IFS= read -r gpu_line; do
        info "found: ${gpu_line}"
    done <<<"$gpu_lines"
fi

preferences_file="$HOME/.config/garage/preferences.toml"

case $gpu_class in
    discrete)
        info "verdict: a discrete GPU is present."
        info "keeping the shipped default, Liquid Glass -- this machine can pay for it."
        ;;
    unknown)
        warn "no GPU-class PCI device could be identified (is lspci available?)."
        info "keeping the shipped default, Liquid Glass, since there is nothing to base a"
        info "  change on. If the desktop stutters, set the window material to Off in"
        info "  System Preferences > Appearance."
        ;;
    integrated)
        info "verdict: integrated graphics only."
        info "Liquid Glass blurs the whole monitor framebuffer for every glass window on"
        info "  every damaged frame, which an integrated GPU cannot keep up with. Frosted"
        info "  is not the cheaper option it looks like -- it flattens the bevel but still"
        info "  captures and blurs the full framebuffer -- so the default here is Off,"
        info "  which skips the plugin's render path entirely."
        if [[ -e $preferences_file || -L $preferences_file ]]; then
            info "your ~/.config/garage/preferences.toml already exists, so your settings are left"
            info "  alone. Set Appearance > Material to Off yourself if the desktop stutters."
        else
            # ---------------------------------------------------------------
            # ONE-WRITER VIOLATION, deliberate, narrow, and measured rather
            # than assumed.
            #
            # `garage` owns preferences.toml and nothing else should write it.
            # This does. The alternative was tried:
            #
            #   `garage set appearance.glass_mode '"off"'` writes the file and
            #   then applies the change, and applying reaches the compositor
            #   through `hyprctl`. From the TTY this bootstrap runs on there is
            #   no compositor, so it exits 1 with a JSON error -- survivable, the
            #   write lands first, and since the deltas-only work `set` writes
            #   only departures, so the old fossilization objection is gone.
            #   What remains is the exit-1-on-a-TTY wart and the apply noise.
            #
            # So: write the two keys directly and leave every other default
            # coming from layer 1, which is what the layering is for.
            #
            # TODO: once `garage` grows a non-applying write path (set --no-apply
            # or similar), this block becomes one call to it.
            # ---------------------------------------------------------------

            # Single-sourced from the tracked defaults rather than hardcoded, so
            # a schema bump cannot leave this writing a stamp that means
            # something else. An unreadable stamp is not fatal: a file without a
            # [schema] section reads as version 1, and garage's migrations for
            # 2, 3 and 4 only touch keys this file does not carry, then stamp it.
            schema_stamp="$(awk -F'[= ]+' '/^preferences_version/ { print $2; exit }' \
                "$repo_dir/desktop/.config/garage/preferences.defaults.toml" 2>/dev/null || true)"
            schema_block=""
            if [[ $schema_stamp =~ ^[0-9]+$ ]]; then
                schema_block="[schema]
preferences_version = ${schema_stamp}

"
            else
                warn "could not read preferences_version from the shipped defaults;"
                warn "  writing preferences.toml unstamped and letting garage migrate it."
            fi

            write_file "$preferences_file" <<PREFERENCES
# Written once by Garage's bootstrap, before the first login, because this
# machine reported integrated graphics only:
#
$(printf '#   %s\n' "${gpu_lines:-none}")
#
# Liquid Glass makes every glass window capture and blur the whole monitor
# framebuffer on every damaged frame. A discrete GPU can pay for that; shared
# memory bandwidth cannot. Off is the only mode that skips the work -- Frosted
# flattens the bevel but still captures and blurs the full framebuffer.
#
# This is a default, not a decision. Change it in System Preferences >
# Appearance, or:
#
#     garage set appearance.glass_mode '"liquid"'
#
# and nothing will overwrite you: bootstrap only writes this file when it does
# not already exist. Every other preference deliberately stays absent here so
# that it keeps coming from the shipped defaults.

${schema_block}[appearance]
glass_mode = "off"
PREFERENCES
            record "set the window material to Off for integrated graphics"
        fi
        ;;
esac

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

user_units=(
    waybar.service    # + ExecStartPre=garage render-bar
    hyprpaper.service # + ExecStartPre=garage render-wallpaper
    hypridle.service  # + ExecStartPre=garage render-idle
    hyprsunset.service
    hyprpolkitagent.service
    swayosd.service
    cliphist.service
    cliphist-image.service
    xsettingsd.service
    garage-plugins-check.service # tells you when a Hyprland update broke the plugins
    garage-shell.service
    garage-theme.timer
    garage-night-shift.timer
)
run systemctl --user enable "${user_units[@]}"
record "enabled ${#user_units[@]} per-user units"

# Nothing renders or applies here. The first full render-and-apply happens at
# the first graphical login, as the `garage apply` in autostart.lua; the
# waybar/hyprpaper/hypridle units each render their own narrow fragment in an
# ExecStartPre on the way up. From a TTY there is no compositor for any of it
# to talk to.

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
#
# Hyprland plugins are ABI-locked to the exact compositor build they were
# compiled against, so any `pacman -Syu` that bumps hyprland invalidates them.
# Three pieces answer that, and all three are installed here rather than at the
# first failure:
#
#   1. /etc/pacman.d/hooks/kinetik-plugins.hook + kinetik-plugin-hook, which run
#      as root at the end of the transaction that moved the ABI. They rebuild
#      nothing -- see the long header in system/bin/kinetik-plugin-hook for why
#      a root-context rebuild is a privilege escalation today -- and instead
#      re-link a build that already exists or unlink one that no longer applies.
#   2. /usr/lib/kinetik/plugin-pins, the pinned commits, published out of the
#      checkout so the hook can read them with no $HOME to look in.
#   3. garage-plugins-check.service, enabled above, which compares the running
#      ABI against what is deployed at each graphical login and notifies.
#
# Installed unconditionally, including on a machine with no plugin source: this
# is the machinery that notices, and it has to be in place before the upgrade
# that needs it, not after.
# ---------------------------------------------------------------------------

step "Installing the plugin ABI hook"
# /usr/lib/kinetik is Garage's own payload root -- the plugins already live
# there. The hook script goes to /usr/local/lib instead, because it is a local
# administrative script that no package owns and /usr/local is where the FHS
# puts those.
run sudo install -d -m 0755 /usr/lib/kinetik /usr/local/lib/kinetik \
    /var/lib/kinetik /etc/pacman.d/hooks
run sudo install -m 0644 "$repo_dir/system/plugin-pins" /usr/lib/kinetik/plugin-pins
run sudo install -m 0755 "$repo_dir/system/bin/kinetik-plugin-hook" \
    /usr/local/lib/kinetik/kinetik-plugin-hook
run sudo install -m 0644 "$repo_dir/system/pacman-hooks/kinetik-plugins.hook" \
    /etc/pacman.d/hooks/kinetik-plugins.hook
record "installed the pacman hook that watches the Hyprland plugin ABI"

step "Deploying the optional Hyprland plugins"
glass_repo=""
for candidate in "$HOME/repositories/glass" "$HOME/repositories/hyprliquid"; do
    [[ -d "$candidate/.git" ]] && {
        glass_repo=$candidate
        break
    }
done

if [[ ${GARAGE_SKIP_PLUGIN_DEPLOY:-0} == 1 ]]; then
    # `garage update` sets this. It makes the plugin decision itself -- comparing
    # the running ABI against what is deployed, and rebuilding only when they
    # disagree or a pin moved in the pull -- because a deploy needs sudo and
    # rebuilds nothing useful when the ABI has not moved. Deploying here as well
    # would do it twice on every update. An install never sets it: a fresh
    # machine has nothing deployed, so the deploy always has work to do.
    info "skipped: GARAGE_SKIP_PLUGIN_DEPLOY=1 (the caller owns the plugin decision)."
    summary+=("left the plugin deploy to the caller")
elif [[ -z $glass_repo ]]; then
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
