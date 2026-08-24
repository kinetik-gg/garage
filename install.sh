#!/bin/sh
#
# Garage installer. Fetches the repository and hands over to bootstrap.sh.
#
#   sh -c "$(curl -fsSL https://raw.githubusercontent.com/kinetik-gg/garage/main/install.sh)"
#   sh -c "$(curl -fsSL .../install.sh)" -- --dry-run

set -eu

repo_url="https://github.com/kinetik-gg/garage.git"
repo_dir="${GARAGE_DIR:-$HOME/repositories/garage}"

[ -e /etc/arch-release ] || {
    echo "Garage installs onto Arch Linux. Install Arch first, then run this." >&2
    exit 1
}

[ "$(id -u)" != 0 ] || {
    echo "Run this as your normal user, not root. It calls sudo where it needs to." >&2
    exit 1
}

command -v sudo >/dev/null || {
    echo "sudo is required, and your user needs to be able to use it." >&2
    exit 1
}

# -Syu rather than -Sy: bootstrap.sh runs a full upgrade as its first step, so
# there is no point leaving the system in a partial-upgrade state until then.
command -v git >/dev/null || sudo pacman -Syu --needed --noconfirm git

if [ -d "$repo_dir/.git" ]; then
    echo "Using the existing checkout at $repo_dir."
elif [ -e "$repo_dir" ]; then
    echo "$repo_dir exists but is not a git checkout. Move it aside and re-run." >&2
    exit 1
else
    mkdir -p -- "$(dirname -- "$repo_dir")"
    git clone "$repo_url" "$repo_dir"
fi

cd -- "$repo_dir"
exec ./bootstrap.sh "$@"
