#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if [[ ${EUID} -eq 0 ]]; then
    echo "Run this as the desktop user, not root." >&2
    exit 1
fi

if ! command -v pacman >/dev/null; then
    echo "This bootstrap targets a bootable vanilla Arch installation." >&2
    exit 1
fi

packages=(
    base-devel git stow cmake meson cpio pkgconf linux-headers
    hyprland uwsm xdg-desktop-portal-hyprland xdg-desktop-portal-gtk
    hypridle hyprlock hyprpaper hyprpolkitagent hyprsunset
    waybar rofi kitty fish fisher
    swaync swayosd cliphist wl-clipboard playerctl
    grim slurp satty hyprpicker
    networkmanager bluez bluez-utils
    pipewire pipewire-alsa pipewire-pulse wireplumber pavucontrol
    nautilus gvfs gvfs-smb gnome-text-editor gnome-calculator loupe
    btop fastfetch micro qt6ct xsettingsd
    python python-pip python-pipx uv jq zenity file libnotify 7zip
    papirus-icon-theme noto-fonts noto-fonts-cjk noto-fonts-emoji ttf-ibm-plex
    ttf-cascadia-mono-nerd awesome-terminal-fonts
    remmina freerdp sddm lm_sensors
    spotify-launcher discord zed
    docker docker-buildx docker-compose
    nodejs npm pnpm bun deno typescript-language-server bash-language-server
    rustup rust-analyzer
    clang lldb gdb ninja ccache mold valgrind perf strace
    shellcheck shfmt ripgrep fd fzf bat eza hyperfine just protobuf
    man-db man-pages desktop-file-utils ffmpeg imagemagick obs-studio
)

if command -v lspci >/dev/null && lspci | grep -qi nvidia; then
    packages+=(nvidia-open nvidia-utils egl-wayland libva-nvidia-driver)
fi

sudo pacman -Syu --needed "${packages[@]}"

sudo systemctl enable NetworkManager.service bluetooth.service sddm.service
sudo systemctl enable --now docker.service
sudo usermod -aG docker "$USER"
sudo usermod -s /usr/bin/fish "$USER"

mkdir -p \
    "${HOME}/Desktop" \
    "${HOME}/Documents" \
    "${HOME}/Downloads" \
    "${HOME}/Music" \
    "${HOME}/Pictures" \
    "${HOME}/Public" \
    "${HOME}/repositories" \
    "${HOME}/Templates" \
    "${HOME}/Videos" \
    "${HOME}/.local/share/wallpaper"

stow --dir="$repo_dir" --target="$HOME" --restow --no-folding desktop

sed "s|@HOME@|${HOME}|g" \
    "$repo_dir/templates/swayosd-config.toml" \
    > "$repo_dir/desktop/.config/swayosd/config.toml"

cat > "$repo_dir/desktop/.config/gtk-3.0/bookmarks" <<BOOKMARKS
file://${HOME}/Documents Documents
file://${HOME}/Downloads Downloads
file://${HOME}/Pictures Pictures
file://${HOME}/repositories Repositories
BOOKMARKS

if [[ ! -e ${HOME}/.local/share/wallpaper/current ]]; then
    ln -s "$repo_dir/desktop/Wallpaper/Dark/Nebula - Martin Martz.jpg" "${HOME}/.local/share/wallpaper/current"
fi

systemctl --user daemon-reload || true

"${HOME}/.local/bin/garage" render

# The vanilla Arch Hyprland profile may install Dunst. This desktop uses SwayNC
# as both the notification daemon and control center, so prevent D-Bus from
# activating Dunst first.
systemctl --user mask --now dunst.service || true

systemctl --user enable \
    waybar.service \
    swayosd.service \
    swaync.service \
    hypridle.service \
    hyprsunset.service \
    garage-night-shift.timer \
    cliphist.service \
    cliphist-image.service \
    hyprpolkitagent.service \
    xsettingsd.service

if fish -c 'type -q fisher'; then
    fish -c 'fisher list | string match -q pure || fisher install pure-fish/pure'
fi

if command -v rustup >/dev/null; then
    rustup default stable
    rustup component add rustfmt clippy
fi

if ! "$HOME/.config/hypr/scripts/garage-rebuild-plugins"; then
    echo "Plugin build failed; Hyprland will still start without optional plugins." >&2
fi

cat <<EOF

Core desktop installation complete.

Before the first graphical login:
  1. Restore the backup-only assets listed in docs/ARCH-REINSTALL.md.
  2. Reboot, then select Hyprland (UWSM) in SDDM.

EOF
