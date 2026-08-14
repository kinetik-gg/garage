# shellcheck shell=bash
# INSTALL.md row 3: install the complete desktop package set.

step "Installing the desktop package set"
run sudo pacman -S --needed "${packages[@]}"
record "installed ${#packages[@]} packages"
