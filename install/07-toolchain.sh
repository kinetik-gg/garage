# shellcheck shell=bash
# INSTALL.md row 7: select stable Rust and build the Garage-themed
# Hyprland polkit authentication modal.

if ((!dry_run)) && ! command -v rustup >/dev/null; then
    fail "rustup is required to build Garage's Rust backend."
fi
run rustup default stable
run rustup component add rustfmt clippy
record "installed the stable Rust toolchain"

# The packaged polkit agent keeps its QML inside the executable, so it cannot be
# themed or replaced from ~/.config. Build the same checksum-pinned upstream
# release with Garage's compact modal and install it in the user's private
# prefix; the tracked systemd drop-in selects this binary without replacing the
# distro package that owns the authentication backend and its dependencies.
run "$repo_dir/system/hyprpolkitagent/build" \
    "$HOME/.local/lib/garage/hyprpolkitagent"
record "built Garage's polkit authentication modal"
