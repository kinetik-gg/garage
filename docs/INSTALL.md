# Install

## Prerequisites

- A bootable, vanilla Arch Linux install (UEFI, networking, `sudo`, and a
  normal user already set up).
- Hyprland is treated as a prerequisite, not something this repository
  provisions from scratch: `bootstrap.sh` installs it and its ecosystem
  packages, but you are expected to start from plain Arch, not a
  pre-configured desktop.

## Install

```sh
git clone https://github.com/kinetik-gg/garage.git ~/repositories/garage
cd ~/repositories/garage
./bootstrap.sh
```

Run this as the desktop user, not root; the script refuses to run as root and
refuses to run anywhere it can't find `pacman`.

## What bootstrap currently does

At a high level, `bootstrap.sh`:

- Installs the desktop package set with `pacman` (Hyprland and its portals,
  Waybar, Quickshell, terminal/shell tooling, and the pinned development
  toolchains), adding NVIDIA packages automatically when it detects NVIDIA
  hardware.
- Enables the core system services (NetworkManager, Bluetooth, Docker, SDDM)
  and puts the user in the `docker` group with `fish` as their shell.
- Symlinks the tracked configuration into `$HOME` with GNU Stow
  (`stow --restow --no-folding desktop`), so everything under `~/.config` and
  `~/.local` that this repo manages is a symlink back into the repository.
- Renders host-specific settings (`garage render`) from the
  settings schema into the generated config fragments Hyprland, Waybar, and
  other consumers read.
- Enables the per-user systemd services the desktop depends on (Waybar,
  SwayOSD, SwayNC, hypridle/hyprsunset, cliphist, the polkit agent, and
  friends), masking `dunst.service` so it doesn't race SwayNC for the
  notification D-Bus name.
- Installs the Pure Fish prompt via Fisher and sets up the Rust toolchain.
- Builds the ABI-pinned Hyprland plugins for the currently running Hyprland
  version (see the plugin rebuild script under `desktop/.config/hypr/scripts`);
  Hyprland's configuration treats both plugins as optional, so a plugin build
  failure does not block the first login.

The settings CLI that renders and manages preferences is `garage`
(`~/.local/bin/garage`), with helper commands prefixed `garage-`.

## Known gaps

The installer is being hardened and is not yet idempotent or defensive in
every case:

- It does not check for pre-existing files at the paths it's about to stow
  over, so an existing dotfile at one of those paths can make `stow` fail
  with a conflict instead of adopting or backing it up.
- Package availability isn't fully guarded; a renamed or removed package in
  the `pacman` list can abort the run partway through.
- Re-running the script is intended to be safe (it's mostly idempotent
  installs and `--restow`), but this hasn't been exercised on every
  supported state.

If a stow conflict blocks you, remove or move aside the conflicting file at
that path and re-run `./bootstrap.sh`. More generally: everything this
repository manages under `~/.config` and `~/.local` is a symlink into the
repository, not a copy — if you ever delete one of those symlinks by mistake,
re-running `stow --restow --no-folding desktop` from the repository root puts
it back without touching anything else.
