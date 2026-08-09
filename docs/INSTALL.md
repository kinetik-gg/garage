# Install

## What Garage expects

Garage installs a whole desktop, starting from a machine that has none. The
target is a **freshly installed, minimal Arch system with no desktop
environment** — the state you are in right after `pacstrap`, a bootloader, and
`useradd`. You run the bootstrap from a bare TTY login; Garage installs Hyprland
itself, along with the display manager, session, bar, shell, and toolchains.

Prerequisites, in full:

- A booting, minimal Arch install. Installing Arch stays your job — Garage is
  not a distribution and ships no ISO.
- A working network connection.
- A normal user account that can use `sudo`. Do not run the bootstrap as root.
- **No** desktop environment, display manager, or AUR helper already set up.

Nothing else. In particular you do **not** need Hyprland, a Wayland session, or
any graphical software installed beforehand — the bootstrap runs entirely from
the text console and never talks to a compositor.

## The freshness policy

**Garage expects a fresh system; it will refuse a used one.**

Before doing anything, `bootstrap.sh` checks for signs that this machine is
already somebody's desktop:

- an enabled `display-manager.service`;
- session files from another desktop in `/usr/share/wayland-sessions` or
  `/usr/share/xsessions` (GNOME, Plasma, Xfce, and friends);
- more than three entries in `~/.config` that `/etc/skel` does not account for;
- `yay` or `paru` on `PATH`.

If any of those hold, it prints what it found and exits without touching the
system. This is not gatekeeping for its own sake: Garage claims `~/.config`
wholesale, enables its own display manager and session, and replaces the
notification daemon. On a machine that already has a desktop those decisions
collide, and an installer cannot adopt another desktop's configuration without
losing part of it.

Re-running the bootstrap on a machine where Garage is *already* installed is a
supported operation and is not blocked.

If you understand the consequences and want to proceed on a used system anyway:

```sh
GARAGE_FORCE=1 ./bootstrap.sh
```

Files at the paths Garage manages are moved to `~/.garage-backup/<timestamp>/`
rather than deleted, but installed packages and enabled services are not
reverted for you.

## Installing

### The one-liner

```sh
sh -c "$(curl -fsSL https://get.kinetik.gg/garage)"
```

**Not live yet.** `get.kinetik.gg` is a placeholder: Garage's repositories are
local-only for now and nothing is published at that address. Use the git path
below until this document says otherwise.

### From a clone

Equally supported, and what the one-liner does for you:

```sh
git clone https://github.com/kinetik-gg/garage.git ~/repositories/garage
cd ~/repositories/garage
./bootstrap.sh
```

`install.sh` in the repository root is the script the one-liner fetches. It
checks that this is Arch, that you are not root, and that `sudo` and `git` are
available, clones (or reuses) `~/repositories/garage`, and hands over to
`./bootstrap.sh`. It passes its arguments straight through, so
`sh -c "$(curl -fsSL …)" -- --dry-run` works.

### Looking before you leap

```sh
./bootstrap.sh --dry-run
```

Prints every mutating action — pacman targets, files it would back up, links it
would create, units it would enable — and executes none of them. A dry run also
reports what the freshness gate found without stopping on it.

## What bootstrap does

In order:

1. **Refuses to continue** if the machine is not fresh (see above), unless
   `GARAGE_FORCE=1`.
2. **Upgrades the system** (`pacman -Syu`), then checks every package name in
   its list against the repositories and reports the whole set of missing names
   at once rather than failing partway through an install.
3. **Installs the package set** — Hyprland and its portals, Quickshell, Waybar,
   Kitty, Fish, Rofi, SwayNC, SwayOSD, PipeWire, the GNOME apps it uses, fonts,
   and the Node/Rust/C++ toolchains. NVIDIA packages are added automatically
   when NVIDIA hardware is detected.
4. **Enables system services**: NetworkManager, Bluetooth, Docker, and SDDM — so
   the reboot at the end lands in a graphical login — and sets `fish` as your
   login shell.
5. **Creates the home directory layout** (`~/Documents`, `~/repositories`, and
   the rest of the XDG set).
6. **Clears the way, then links the configuration.** It works out exactly what
   `stow` is about to claim, moves any pre-existing real file at those paths to
   `~/.garage-backup/<timestamp>/`, deletes stale links left behind by a Garage
   checkout that has since moved, and only then runs
   `stow --restow --no-folding desktop`. If `stow` still reports a conflict, the
   run stops with the list rather than leaving your home half-linked.
7. **Writes the two per-user generated files** — `~/.config/swayosd/config.toml`
   and `~/.config/gtk-3.0/bookmarks`. These embed an absolute `$HOME`, so they
   are real files rather than links into the repository, and they are only
   written when absent: your edits survive a re-run.
8. **Enables the per-user services** — Waybar, hypridle, hyprpaper, hyprsunset,
   the polkit agent, SwayNC, SwayOSD, cliphist, xsettingsd, the Garage shell,
   and the theme and Night Shift timers — and masks `dunst.service` so D-Bus
   cannot activate it ahead of SwayNC. Nothing is *started*: every unit is
   wanted by `graphical-session.target` (or `timers.target`) and comes up at
   your first graphical login.
9. **Installs the Pure Fish prompt** at a pinned tag and the stable Rust
   toolchain.
10. **Deploys the optional Hyprland plugins**, if their source is present.
11. **Prints a summary and tells you to reboot.**

## Why bootstrap does not render your settings

`garage render` is deliberately not called during the bootstrap. Rendering
reloads Hyprland through `hyprctl`, which cannot work with no compositor
running, so from a TTY the render has nothing sensible to do and errors out.

The first full render happens at your first graphical login instead, as
`hypridle.service`'s `ExecStartPre` (see
`desktop/.config/systemd/user/hypridle.service.d/garage.conf`). Waybar and
hyprpaper render their own fragments the same way, through `garage render-bar`
and `garage render-wallpaper`. This is why the first login takes a few seconds
longer than the ones after it.

The settings CLI that renders and manages preferences is `garage`
(`~/.local/bin/garage`), with helper commands prefixed `garage-`.

## Docker

Garage installs and enables Docker but deliberately does **not** add you to the
`docker` group. Membership in that group is equivalent to passwordless root on
this machine, and that is not a trade an installer should make on your behalf.
Use `sudo docker`, or opt in yourself if you have decided you want it:

```sh
sudo usermod -aG docker "$USER"   # then log out and back in
```

## After the reboot

SDDM starts on the next boot. Pick **Hyprland (UWSM)** and log in.

## Known gaps

Honest state of the installer today:

- **The one-liner is not live.** Garage's repositories are local-only; the
  `github.com/kinetik-gg` URLs in `install.sh` and in the plugin deploy script
  are dormant placeholders. Until they are published, install from a local
  clone.
- **The Glass plugin source is not available.** With no `~/repositories/glass`
  checkout, the bootstrap warns and skips the plugin deploy. Hyprland's config
  guards both plugin loads, so the desktop comes up without them — you lose the
  glass material and `hyprexpo`, nothing else. `hyprexpo` is skipped along with
  it, because the deploy script handles both in one pass.
- **A forced install on a used system is not reversible.** Managed paths are
  backed up, but packages and enabled services are not rolled back.
- **`--dry-run` reports package names against the current sync database.** On a
  never-synced system the name check is skipped with a warning, since there is
  nothing to check against yet.
- **Re-running is intended to be safe** — the installs are `--needed`, the link
  step is `--restow`, and the generated files are only written when absent — but
  it has not been exercised from every possible starting state.

If a `stow` conflict stops the run, each line names a path that already exists
and is not a Garage link. Move it aside and re-run `./bootstrap.sh`. More
generally: everything Garage manages under `~/.config` and `~/.local` is a
symlink into the repository, not a copy. If you delete one by mistake, re-running
`stow --restow --no-folding desktop` from the repository root puts it back
without touching anything else.
