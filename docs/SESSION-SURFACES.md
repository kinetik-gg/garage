# Sign-in and lock surfaces

Garage owns two different authentication surfaces and deliberately keeps their
responsibilities separate:

- **SDDM** signs a user into a session. Its root-owned theme lives under
  `system/sddm/garage`, remembers SDDM's last user and session, and exposes a
  switch-user field, session selection, sleep, reboot and shutdown. Reboot and
  shutdown require inline confirmation; sleep is immediate.
- **Hyprlock** unlocks the already-running session. It follows Garage's current
  wallpaper and generated light/dark palette. Its only control is a transparent
  password field, placed by `garage-lock-session` on Hyprland's focused monitor
  while the wallpaper remains on every connected output.

Both surfaces keep authentication/error feedback inside fixed geometry. A
failed password therefore changes text and colour without moving the controls.

## SDDM deployment

`system/sddm/install` stages the QML, SVGs, Plus Jakarta Sans, its OFL text, and
Garage's fixed dark Unsplash wallpaper. It publishes a content-addressed
root-owned directory and atomically flips `/usr/share/sddm/themes/garage` before
installing `/etc/sddm.conf.d/10-garage.conf`. Bootstrap does this before it
enables `sddm.service`.

The greeter remains X11. The selected desktop session remains SDDM's normal
Wayland session choice; on a new Garage install the first sorted session is
`hyprland-uwsm.desktop`, while an existing SDDM choice remains respected.

The bundled SDDM wallpaper is intentionally fixed and dark because the greeter
runs as the isolated `sddm` user and must not read a desktop user's home. The
theme metadata includes the wallpaper attribution, and avatars are disabled so
the greeter does not probe user homes for face files.

## Hyprlock routing

Before invoking Hyprlock, `garage-lock-session` reads `hyprctl monitors -j`:

1. use the focused output;
2. otherwise use the first connected output;
3. otherwise leave the monitor empty, which is Hyprlock's all-monitor fallback.

Only conservative output names are accepted before writing Hyprlang. The
include is written to a temporary file with mode `0600` and atomically renamed
to `~/.local/state/garage/generated/hyprlock-monitor.conf`. Notification
inhibition and release still wrap the whole lock lifetime.

## Safe verification

Run:

```sh
python3 tests/run
```

The tests load the actual SDDM QML with Qt 6's `sddm-greeter-qt6 --test-mode`
using the offscreen backend. Power methods do nothing in test mode. Hyprlock's
monitor selection is exercised with a mocked compositor and mocked lock binary.
These checks do not restart SDDM, lock the current session, or invoke a power
action.
