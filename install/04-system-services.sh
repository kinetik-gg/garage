# shellcheck shell=bash
# INSTALL.md row 4: install the sign-in theme, enable system
# services, and select the user's login shell.

# ---------------------------------------------------------------------------
# System services and the login shell
# ---------------------------------------------------------------------------

# SDDM starts at the next boot, but the service must never be enabled before its
# complete theme, font, wallpaper and configuration have been published. Keep
# sudo inside run() so --dry-run remains side-effect-free while the installer
# itself stays usable as a root-owned deployment primitive.
step "Installing the Garage sign-in theme"
run sudo "$repo_dir/system/sddm/install"
record "installed Garage's SDDM theme"

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
