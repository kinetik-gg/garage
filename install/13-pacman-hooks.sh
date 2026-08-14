# shellcheck shell=bash
# INSTALL.md row 13: install the plugin ABI and reconciliation stamp
# pacman hooks.

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

step "Installing the reconciliation stamp hook"
# The pacman action is deliberately only `touch`, so tmpfiles owns the runtime
# directory across boots. Create it now as well: the first transaction after a
# bootstrap must not have to wait for a reboot before its hook is valid.
run sudo install -d -m 0755 /usr/lib/tmpfiles.d /etc/pacman.d/hooks
run sudo install -m 0644 "$repo_dir/system/tmpfiles.d/garage.conf" \
    /usr/lib/tmpfiles.d/garage.conf
run sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/garage.conf
run sudo install -m 0644 "$repo_dir/system/pacman-hooks/garage-reconcile.hook" \
    /etc/pacman.d/hooks/garage-reconcile.hook
record "installed the pacman hook that schedules Garage reconciliation"
