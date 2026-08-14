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

source "$repo_dir/install/lib.sh"

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

stages=("$repo_dir"/install/[0-9][0-9]-*.sh)
if ((${#stages[@]} < 14)); then
    fail "this checkout is missing its install stages (found ${#stages[@]} under install/)."
fi
for stage in "${stages[@]}"; do
    # shellcheck source=/dev/null
    source "$stage"
done

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
