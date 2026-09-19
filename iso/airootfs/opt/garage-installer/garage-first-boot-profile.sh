# Garage ISO handoff. This file is inert after the one pending run is claimed.
garage_installer_user="$(cat /etc/garage-installer-user 2>/dev/null)"
garage_installer_state="$HOME/.local/state/garage-installer"

if [ "$(tty 2>/dev/null)" = /dev/tty1 ] && \
    [ "$(id -un)" = "$garage_installer_user" ] && \
    [ -f "$garage_installer_state/pending" ]; then
    /usr/local/lib/garage/first-boot
fi

unset garage_installer_user garage_installer_state
