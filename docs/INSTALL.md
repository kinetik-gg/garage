# Install

## What Garage expects

Garage installs a whole desktop onto a machine that has none — Hyprland, display
manager, session, bar, shell, toolchains — driven from a bare TTY. The target is a
**freshly installed, minimal Arch system with no desktop environment**: the state
right after `pacstrap`, a bootloader, and `useradd`. Prerequisites, in full:

- A booting, minimal Arch install. Installing Arch stays your job — Garage is not a
  distribution and ships no ISO.
- A working network connection.
- A normal user account that can use `sudo`. Do not run the bootstrap as root.
- **No** desktop environment, display manager, or AUR helper already set up.

## The freshness policy

**Garage expects a fresh system; it will refuse a used one.** `bootstrap.sh` first
looks for signs of an existing desktop: an enabled `display-manager.service`;
another desktop's session files in `/usr/share/wayland-sessions` or
`/usr/share/xsessions` (GNOME, Plasma, Xfce); more than three entries in
`~/.config` unaccounted for by `/etc/skel`; `yay` or `paru` on `PATH`. If any hold,
it prints what it found and exits without touching the system. Garage claims
`~/.config` wholesale, enables its own display manager and session, and replaces
the notification daemon — decisions that collide with an existing desktop, whose
configuration an installer cannot adopt without losing part of it.

**Re-running where Garage is already installed is supported and not blocked**: the
gate recognises an existing install, `pacman` runs with `--needed`, the link step
with `--restow`, and mutable per-user files are written only when absent. Managed
artifacts and release binaries are rebuilt so the installed code converges; host
preferences and user-owned files are not overwritten.

To proceed on a used system anyway:

```sh
GARAGE_FORCE=1 ./bootstrap.sh
```

Managed paths move to `~/.garage-backup/<timestamp>/` rather than being deleted;
packages and enabled services are not reverted.

## Installing

### From a clone

Clone the public repository and run the bootstrap from a bare TTY:

```sh
git clone https://github.com/kinetik-gg/garage.git ~/repositories/garage
cd ~/repositories/garage
./bootstrap.sh
```

`install.sh` is the remote-entry wrapper: it checks that this is Arch, that you
are not root, and that `sudo` and `git` are available, clones (or reuses)
`~/repositories/garage`, and hands over to `./bootstrap.sh` with its arguments
passed through.

### Looking before you leap

```sh
./bootstrap.sh --dry-run
```

Prints every mutating action — pacman targets, files backed up, links created,
units enabled — and executes none. It reports what the freshness gate found
without stopping on it, and checks package names against the current sync database
(skipped with a warning on a never-synced system).

## What bootstrap does

| # | Step | File |
| --- | --- | --- |
| 1 | **Refuses to continue** unless the machine is fresh (above) or `GARAGE_FORCE=1`. | `install/01-freshness-gate.sh` |
| 2 | **Upgrades the system** (`pacman -Syu`), then checks its whole package list against the repositories, reporting every missing name at once rather than failing partway through an install. | `install/02-package-database.sh` |
| 3 | **Installs the package set**: Hyprland and its portals, Quickshell, Kitty, Fish, PipeWire, brightness control, clipboard history, Thunar and its file integrations, the GNOME utility apps it uses, fonts, and the Node/Rust/C++ toolchains — plus NVIDIA packages when NVIDIA hardware is detected. | `install/03-packages.sh` |
| 4 | **Installs the sign-in surface, then enables system services.** The complete root-owned SDDM theme is staged and atomically published before NetworkManager, Bluetooth, Docker, and SDDM are enabled, so the reboot lands in the Garage login screen. Sets `fish` as your login shell. | `install/04-system-services.sh` |
| 5 | **Creates the home directory layout** (`~/Documents`, `~/repositories`, the rest of the XDG set). | `install/05-home-layout.sh` |
| 6 | **Clears the way, then links the configuration.** It moves pre-existing real files at the paths `stow` will claim to `~/.garage-backup/<timestamp>/`, deletes stale links from a checkout that has since moved, then runs `stow --restow --no-folding desktop`; a remaining conflict stops the run with the list rather than leaving your home half-linked. The linked wallpaper set is the 4K production output under `desktop/Wallpaper/`; originals and provenance stay outside Stow under `assets/wallpapers/`. `fc-cache` follows, so the bundled fonts (Phosphor, Plus Jakarta Sans, Geist Mono, linked into `~/.local/share/fonts` by the same pass) are ready at first login. It seeds Hyprlock's all-monitor fallback and builds the Thunar-only GTK module. | `install/06-link-config.sh` |
| 7 | **Selects the stable Rust toolchain, then builds Garage's authentication modal** from the checksum-pinned `hyprpolkitagent` 0.1.3 source into the user's private prefix. | `install/07-toolchain.sh` |
| 8 | **Builds and installs the Rust backend.** One release build produces `garage`, `garage-metrics`, `garage-file-index`, `garage-ai-usage`, and `garage-bar-probe`; the executable copies land in `~/.local/lib/garage/bin`, with command symlinks under `~/.local/bin`. | `install/08-backend.sh` |
| 9 | **Writes the per-user generated files**: `~/.config/gtk-3.0/bookmarks` and Thunar's first-run xfconf layout. Mutable or embedding an absolute `$HOME`, they are real files rather than links, so changing columns, geometry, or bookmarks does not dirty Garage. | `install/09-per-user-files.sh` |
| 10 | **Picks a window material your GPU can afford** (below); with a discrete GPU it writes nothing at all. | `install/10-window-material.sh` |
| 11 | **Enables the per-user services** — hypridle, hyprpaper, hyprsunset, the polkit agent, cliphist, xsettingsd, the Garage shell, background file indexing, the plugin ABI check, and the theme and Night Shift timers — and masks `dunst.service` so D-Bus cannot activate it ahead of the Garage shell's notification daemon. Nothing is *started*: each unit is wanted by `graphical-session.target` or `timers.target` and comes up at your first graphical login. | `install/11-user-services.sh` |
| 12 | **Installs the Pure Fish prompt** at a pinned tag. | `install/12-shell-prompt.sh` |
| 13 | **Installs the pacman hooks**: the plugin ABI hook, the root script it runs, and the pinned plugin commits (below), plus the reconciliation stamp hook. The ABI machinery is installed always, even with no plugin source present: it is the part that notices a broken plugin, so it must be in place before the upgrade that breaks one. | `install/13-pacman-hooks.sh` |
| 14 | **Deploys the optional Hyprland plugins**, if their source is present. | `install/14-plugins.sh` |
| 15 | **Prints a summary and tells you to reboot.** SDDM starts on the next boot; pick **Hyprland (UWSM)** and log in. | `bootstrap.sh` |

## Why bootstrap does not render your settings

Nothing renders or applies during the bootstrap: from a TTY there is no compositor
for the applied half to talk to. The first full render-and-apply happens at your
first graphical login, driven by the `garage apply` in Hyprland's autostart; the
hyprpaper and hypridle units each render their own fragment in an
`ExecStartPre` (`garage render-wallpaper`,
`garage render-idle`) on the way up — which is why the first login takes a few
seconds longer than the ones after it. The release binaries live in
`~/.local/lib/garage/bin`; `~/.local/bin/garage` and the four backend helper names are
symlinks into that private install, so units and interactive shells use stable command paths.
SDDM and Hyprlock ownership, monitor routing, power confirmation, and safe test
procedures are documented in [SESSION-SURFACES.md](SESSION-SURFACES.md).

## The window material gate

Liquid Glass — `glass_mode = "liquid"`, what Garage ships — is expensive: for every
glass window, on every damaged frame, the plugin captures the whole monitor
framebuffer, downscales it and runs a multi-pass blur. A discrete GPU does not
notice; integrated graphics, sharing memory bandwidth with the CPU, cannot keep up,
and a first login that stutters reads as a broken desktop rather than a setting. So
the bootstrap picks the default from your hardware before anything renders,
printing the devices found and the verdict either way:

| Hardware found | What it writes |
| --- | --- |
| A discrete GPU (NVIDIA, an AMD card, Intel Arc) | Nothing. Liquid Glass stays. |
| Integrated graphics only | A minimal `~/.config/garage/preferences.toml` holding exactly a schema stamp and `glass_mode = "off"`; every other preference stays absent and keeps coming from the shipped defaults. |
| Nothing identifiable (no `lspci`, no GPU-class PCI device) | Nothing, and it says so: no evidence to act on. |

**Off, not Frosted**: Frosted looks like the cheap middle setting but is not one —
it flattens the bevel and still captures and blurs the full framebuffer. Only Off
skips the plugin's render path, the part that costs. And a **default, not a
decision**: change it under System Preferences → Appearance or with
`garage set appearance.glass_mode '"liquid"'`. A re-run never overwrites your
choice (see [the freshness policy](#the-freshness-policy)).

## Day two: what happens when Hyprland updates

Hyprland plugins are locked to the exact compositor build they were compiled
against: the ABI string is Hyprland's own commit plus the versions of five `hypr*`
libraries, so any `pacman -Syu` that bumps `hyprland` invalidates the deployed
Kinetik Glass and `hyprexpo` builds at a stroke. Three places handle that, none of
which can cost you your desktop:

| Guard | What it does |
| --- | --- |
| **Hyprland's config** | `hyprland.lua` wraps each `hl.plugin.load` in a `pcall`, so a plugin that cannot load costs you the glass material and nothing else; binds, window rules and monitors still apply. |
| **A pacman hook** | After a transaction that installs or upgrades `hyprland`, `/etc/pacman.d/hooks/kinetik-plugins.hook` runs `/usr/local/lib/kinetik/kinetik-plugin-hook` as root. It builds nothing — a multi-minute compile inside your transaction would be slow and fragile — and instead re-points a plugin at a build already present for the new ABI (making a downgrade, or a return to a version you built for before, instant and silent), or unlinks the stale one and prints what to run into pacman's output. It always exits successfully: no plugin question is worth a failed transaction. |
| **A login-time check** | `garage-plugins-check.service` runs `garage-rebuild-plugins --check` once your session is up, comparing the running ABI against what is deployed — live, so it cannot go stale or be missed — and notifies you if they disagree. Silent otherwise, never blocks login, says nothing on a machine that never had the plugins. |

To bring them back, run `~/.config/hypr/scripts/garage-rebuild-plugins`: it asks
for `sudo` once, since it installs into `/usr/lib/kinetik/plugins`, then tells you
to reload Hyprland. Unlinked builds are not deleted —
`/usr/lib/kinetik/plugins/<abi>/` keeps every plugin you have ever deployed, so
returning to a Hyprland you have used before needs no rebuild at all.

**Why isn't this automatic?** It would mean building Kinetik Glass as root out of
`~/repositories/glass`, a directory you can write to — anything running as you
could edit the build files and have root run them at the next upgrade; a
passwordless `sudo` rule has the same hole ([ARCHITECTURE.md](ARCHITECTURE.md) §6
owns this reasoning). Moving the rebuild into a system service would first require
a root-owned source checkout verified against its pinned commit; until then it is
one command, once, after an upgrade that moved the ABI.

### The pinned commits

Both plugins are pinned in exactly one tracked file, `system/plugin-pins`. The
bootstrap publishes a copy to `/usr/lib/kinetik/plugin-pins` because the pacman
hook runs as root with no checkout and no `$HOME` to read them from;
`garage-rebuild-plugins` re-publishes that copy whenever the tracked one changes,
so bumping a plugin stays a one-file edit.

## Day two: taking a new version of Garage

```sh
garage update            # pull, relink, converge, reload
garage update --dry-run  # print all of that and do none of it
```

After checking free space, a real update copies Garage's four user-owned files
(`preferences.toml`, `displays.toml`, `keybindings.toml`, and
`workspace-blocks.toml`) to
`~/.garage-backup/<timestamp>/pre-update/` before it changes the home or the
system. The same directory records which files were absent, the checkout commit
from before the pull, and the explicitly installed package list. Generated
fragments are not copied: they are a cache rebuilt by the render step.

To restore, replace `<newest>` below with the newest timestamped directory, copy
its backed-up TOML files to `~/.config/garage/`, then apply them:

```sh
backup_dir="$HOME/.garage-backup/<newest>/pre-update"
cp "$backup_dir"/*.toml "$HOME/.config/garage/"
garage apply
```

The whole lifecycle in one command, deliberately **bootstrap plus the lifecycle
work bootstrap cannot do**:

1. **Pull the checkout.** Fast-forward only. Skipped with a note when the branch
   has no upstream or when the working tree has local changes, because merging
   across those is the one operation that loses work.
2. **Check room and preserve the host.** Update refuses a real run below its
   free-space floor, then makes the complete `pre-update` copy described above.
   A dry run crosses neither write boundary.
3. **Sweep links to files the new version deleted.** `stow --restow` only unlinks
   what the package still contains, and bootstrap's pre-stow scan walks the paths
   the checkout manages *now*, so a file deleted between versions leaves a live
   symlink neither will ever look at. Update finds it from the other end: every
   symlink under `~/.config`, `~/.local/bin`, `~/.local/share` and `~/Wallpaper`
   that points into this checkout and no longer resolves — scoped to those four
   because `$HOME` is full of symlinks that dangle legitimately, and a broader
   sweep would eventually delete one.
4. **Re-run `bootstrap.sh`**, which installs packages the list has gained, enables
   units it has gained, rebuilds and installs the five Rust backend binaries, writes
   new per-user files, backs up anything in the way, and restows — safely, for the reasons in
   [the freshness policy](#the-freshness-policy). This is also why update asks for
   `sudo` and can upgrade the system: installing a newly listed package on Arch
   without a full upgrade first is a partial upgrade, which is unsupported.
5. **Render and reload.** The render runs any preference-schema migration the new
   version added, then `hyprctl reload` picks it all up. With no compositor
   reachable — a TTY, an SSH shell — the reload is skipped with a note and the new
   configuration lands at the next login.

Plugins are the one thing update decides for itself: it compares the running
Hyprland ABI against what is deployed and rebuilds only when they disagree or a pin
moved in the pull, so the common update rebuilds nothing and needs no `sudo` for
the plugins.

## Checking an install: `garage doctor`

`garage doctor` is read-only and safe to run anywhere at any time, including a TTY
after a login that did not come up. One line per check, exit 1 if anything is
actually wrong: the Hyprland version against the support floor, the key packages,
the bundled font families (Plus Jakarta Sans, Geist Mono) as fontconfig sees them,
every path this checkout manages resolving from `$HOME` into it, dangling links
left by a deleted file, the per-user units, the plugin ABI, the generated Lua
fragments (each run through `luac -p`), and the preferences file. Three are
reported as `note` rather than a failure, being true and not problems: no plugins
deployed, a home that has never rendered, and a shell with no compositor to reach.
A health check that fails on a TTY is a health check nobody runs.

## Docker

Garage installs and enables Docker but deliberately does **not** add you to the
`docker` group: membership there is equivalent to passwordless root on this
machine, and that is not a trade an installer should make on your behalf. Use
`sudo docker`, or opt in yourself with `sudo usermod -aG docker "$USER"` and log
out and back in.

## Known gaps

| Gap | Detail |
| --- | --- |
| **The Glass plugin source checkout is optional** | With no `~/repositories/glass` checkout the bootstrap warns and skips the plugin deploy — `hyprexpo` with it, since one pass handles both. Hyprland's config guards both loads, so the desktop comes up without them: you lose the glass material and `hyprexpo`, nothing else. |
| **The GPU verdict is a heuristic** | It reads PCI vendor ids and device names: NVIDIA is always discrete, Intel integrated unless it names itself Arc, an AMD device discrete when it carries a card model number or a known GPU family name. A new AMD part breaking that pattern would be misclassified — one setting either way, not a broken install. Virtual adapters (virtio, QXL) count as integrated, deliberately: software rendering is the last place to run a full-framebuffer blur. |
| **The bootstrap writes `preferences.toml` directly for the GPU gate** | Every other writer of that file is `garage` itself, as it should be, but the CLI cannot write a preference without also pushing it into a running compositor and there is none during the bootstrap — so this write goes around it, keeps to two keys, and is marked in the source for the render/apply split to remove. |
| **The plugin ABI hook watches `hyprland` only** | A machine on `hyprland-git` is on its own; the hook will not fire for it. |
| **A forced install on a used system is not reversible** | Managed paths are backed up; packages and enabled services are not rolled back. |
| **Re-running has not been exercised from every starting state** | It is intended to be safe (see [the freshness policy](#the-freshness-policy)), but has not been proven from every one. |

If a `stow` conflict stops the run, each line names a path that already exists and
is not a Garage link: move it aside and re-run `./bootstrap.sh`. Everything Garage
tracks under `~/.config` and `~/.local` is linked from the repository; rendered files,
mutable per-user files, and bootstrap-built artifacts are the documented real-file
exceptions, while the backend commands link to `~/.local/lib/garage/bin`. Delete a tracked
link by mistake and `stow --restow --no-folding desktop` from the repository root puts it
back without touching anything else.
