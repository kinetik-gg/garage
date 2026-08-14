# shellcheck shell=bash
# INSTALL.md row 5: create the home and XDG directory layout.

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
