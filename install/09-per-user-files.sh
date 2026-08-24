# shellcheck shell=bash
# INSTALL.md row 9: write mutable per-user configuration files and
# select the initial wallpaper.

# ---------------------------------------------------------------------------
# Per-user generated files
#
# The GTK bookmark list carries an absolute $HOME, so it cannot be tracked. It
# is written as a real file directly into ~/.config, after stow, and only when
# absent -- your edits survive a re-run. Older bootstraps wrote it into the
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

# Xfconf files are mutable application state. Linking this into the checkout
# would make changing a column width or closing the sidebar dirty the Garage
# repository, so seed a polished first-run layout and let Thunar own the copy.
# Existing installs win: bootstrap never replaces their chosen view or geometry.
thunar_config="$HOME/.config/xfce4/xfconf/xfce-perchannel-xml/thunar.xml"
if [[ ! -e $thunar_config && ! -L $thunar_config ]]; then
    run mkdir -p -- "$(dirname -- "$thunar_config")"
    run cp -- "$repo_dir/templates/thunar.xml" "$thunar_config"
    record "seeded Garage's first-run Thunar layout"
else
    info "keeping the existing Thunar layout"
fi

if [[ ! -e "${HOME}/.local/share/wallpaper/current" ]]; then
    run ln -s "$repo_dir/desktop/Wallpaper/Dark/rMRT4hF-Fsg.jpg" \
        "${HOME}/.local/share/wallpaper/current"
    record "selected the default wallpaper"
fi
