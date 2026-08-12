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
   Kitty, Fish, Rofi, SwayOSD, PipeWire, Thunar and its file integrations,
   the GNOME utility apps it uses, fonts,
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
   run stops with the list rather than leaving your home half-linked. It then
   runs `fc-cache` so the bundled fonts -- Phosphor, Plus Jakarta Sans, and
   Geist Mono, linked into `~/.local/share/fonts` by the same stow pass -- are
   ready for the bar (and everything else) at first login.
   It also builds Garage's authentication modal from the checksum-pinned
   `hyprpolkitagent` 0.1.3 backend into the user's private prefix.
7. **Writes the per-user generated files** — `~/.config/swayosd/config.toml`,
   `~/.config/gtk-3.0/bookmarks`, and Thunar's first-run xfconf layout. These
   are mutable or embed an absolute `$HOME`, so they are real files rather than
   links into the repository, and they are only written when absent: changing
   columns, geometry, bookmarks, or the OSD does not dirty Garage, and your
   edits survive a re-run.
8. **Picks a window material your GPU can afford** (see below). On a machine
   with a discrete GPU it writes nothing at all.
9. **Enables the per-user services** — Waybar, hypridle, hyprpaper, hyprsunset,
   the polkit agent, SwayOSD, cliphist, xsettingsd, the Garage shell,
   background file indexing, the plugin ABI check, and the theme and Night Shift timers — and masks
   `dunst.service` so D-Bus cannot activate it ahead of the Garage shell's
   notification daemon. Nothing is
   *started*: every unit is wanted by `graphical-session.target` (or
   `timers.target`) and comes up at your first graphical login.
10. **Installs the Pure Fish prompt** at a pinned tag and the stable Rust
    toolchain.
11. **Installs the plugin ABI hook** — a pacman hook, the root script it runs,
    and the pinned plugin commits (see below). Always, even on a machine with no
    plugin source: it is the part that notices a broken plugin, so it has to be
    in place before the upgrade that breaks one.
12. **Deploys the optional Hyprland plugins**, if their source is present.
13. **Prints a summary and tells you to reboot.**

## Why bootstrap does not render your settings

Nothing renders or applies during the bootstrap — from a TTY there is no
compositor for the applied half to talk to. The first full render-and-apply
happens at your first graphical login, driven by the `garage apply` in
Hyprland's autostart; the waybar, hyprpaper and hypridle units each render
their own narrow fragment in an `ExecStartPre` (`garage render-bar`,
`garage render-wallpaper`, `garage render-idle`) on the way up. This is why
the first login takes a few seconds longer than the ones after it.

The settings CLI that renders and manages preferences is `garage`
(`~/.local/bin/garage`), with helper commands prefixed `garage-`.

## The window material gate

Garage ships `glass_mode = "liquid"`, and Liquid Glass is expensive in a specific
way: for every glass window, on every damaged frame, the plugin captures the
whole monitor framebuffer, downscales it and runs a multi-pass blur over it. A
discrete GPU does not notice. Integrated graphics, sharing memory bandwidth with
the CPU, cannot keep up, and a first login that stutters reads as a broken
desktop rather than as a setting.

So the bootstrap decides the default from your hardware, before anything
renders:

- **A discrete GPU** (NVIDIA, an AMD card, Intel Arc) — nothing is written.
  Liquid Glass stays.
- **Integrated graphics only** — it writes a minimal
  `~/.config/garage/preferences.toml` containing exactly a schema stamp and
  `glass_mode = "off"`. Every other preference stays absent so it keeps coming
  from the shipped defaults.
- **Nothing identifiable** (no `lspci`, no GPU-class PCI device) — nothing is
  written, and it says so. There is no evidence to act on.

It prints the devices it found and the verdict either way.

Two honest details. **Off, not Frosted**: Frosted looks like the cheap middle
setting but is not one — it flattens the bevel and still captures and blurs the
full framebuffer. Only Off skips the plugin's render path, which is the part that
costs. And this is a **default, not a decision**: change it under System
Preferences → Appearance, or

```sh
garage set appearance.glass_mode '"liquid"'
```

Bootstrap only writes that file when it does not already exist, so a re-run never
overwrites your choice.

## Day two: what happens when Hyprland updates

Hyprland plugins are locked to the exact compositor build they were compiled
against — the ABI string is Hyprland's own commit plus the versions of five
`hypr*` libraries. So any `pacman -Syu` that bumps `hyprland` invalidates the
deployed Kinetik Glass and `hyprexpo` builds at a stroke. Garage handles that in
three places, and none of them can cost you your desktop:

1. **Hyprland's config guards the load.** `hyprland.lua` wraps each
   `hl.plugin.load` in a `pcall`, so a plugin that cannot load leaves you without
   the glass material and nothing else. The rest of the configuration — binds,
   window rules, monitors — still applies.
2. **A pacman hook does the bookkeeping.** After a transaction that installs or
   upgrades `hyprland`, `/etc/pacman.d/hooks/kinetik-plugins.hook` runs
   `/usr/local/lib/kinetik/kinetik-plugin-hook` as root. It builds nothing — it
   is inside your transaction, and a multi-minute compile there would be both
   slow and fragile. It either re-points a plugin at a build already present for
   the new ABI (which is what makes a downgrade, or an upgrade back to a version
   you have built for before, instant and silent), or unlinks the stale one and
   prints a line into pacman's output telling you what to run. It always exits
   successfully: no plugin question is worth a failed transaction.
3. **A login-time check tells you.** `garage-plugins-check.service` runs
   `garage-rebuild-plugins --check` after your session comes up. It compares the
   running ABI against what is deployed — a live comparison, so it cannot go
   stale or be missed — and notifies you if they disagree. It is silent
   otherwise, never blocks login, and on a machine that never had the plugins it
   says nothing at all.

To bring the plugins back:

```sh
~/.config/hypr/scripts/garage-rebuild-plugins
```

It asks for `sudo` once, since it installs into `/usr/lib/kinetik/plugins`, then
tells you to reload Hyprland.

**Why isn't this automatic?** Because doing it for you means building Kinetik
Glass as root out of `~/repositories/glass`, a directory you can write to. That
is a local root escalation dressed up as a convenience: anything running as you
could edit the build files and have root run them at the next upgrade. A
passwordless `sudo` rule for the rebuild script has the same problem. Once Glass
is published and a root-owned clone can be verified against its pinned commit,
the rebuild can move into a system service and this becomes invisible. Until
then it is one command, once, after an upgrade that moved the ABI.

The unlinked build is not deleted: `/usr/lib/kinetik/plugins/<abi>/` keeps every
plugin you have ever deployed, which is why going back to a Hyprland you have
used before needs no rebuild at all.

### The pinned commits

Both plugins are pinned, and the pins live in exactly one tracked file,
`system/plugin-pins`. The bootstrap publishes a copy to
`/usr/lib/kinetik/plugin-pins` because the pacman hook runs as root with no
checkout and no `$HOME` to read them from; `garage-rebuild-plugins` re-publishes
that copy whenever the tracked one changes, so bumping a plugin stays a one-file
edit.

## Day two: taking a new version of Garage

```sh
garage update            # pull, relink, converge, reload
garage update --dry-run  # print all of that and do none of it
```

`garage update` is the whole lifecycle in one command, and it is deliberately
**bootstrap plus two things bootstrap cannot do**:

1. **Pull the checkout.** Fast-forward only, skipped with a note when the branch
   has no upstream — which is every install today, because Garage's repositories
   are local-only — and skipped when the working tree has local changes, because
   merging across those is the one operation that loses work.
2. **Sweep links to files the new version deleted.** `stow --restow` can only
   unlink what the package still contains, and bootstrap's pre-stow scan walks
   the paths the checkout manages *now*, so a file deleted between two versions
   leaves a live symlink that neither of them will ever look at. Update finds it
   from the other end: every symlink under `~/.config`, `~/.local/bin`,
   `~/.local/share` and `~/Wallpaper` that points into this checkout and no longer
   resolves. Scoped to those four directories on purpose — `$HOME` is full of
   symlinks that dangle legitimately, and a broader sweep would eventually delete
   one.
3. **Re-run `bootstrap.sh`.** Which is what installs packages the list has gained,
   enables units it has gained, writes new per-user files, backs up anything in
   the way, and restows. Re-running it is a supported operation: the freshness
   gate recognises an existing install, `pacman` runs with `--needed`, and the
   generated files are only written when absent. This is also why update asks for
   `sudo` and can upgrade the system — installing a newly listed package on Arch
   without a full upgrade first is a partial upgrade, which is unsupported.
4. **Render and reload.** The render runs any preference-schema migration the new
   version added, then `hyprctl reload` picks it all up. With no compositor
   reachable — a TTY, an SSH shell — the reload is skipped with a note and the new
   configuration lands at the next login.

The plugins are the one thing update decides for itself rather than leaving to
bootstrap: it compares the running Hyprland ABI against what is deployed and
rebuilds only when they disagree or a pin moved in the pull. So the common update
does not rebuild anything and does not need `sudo` for the plugins.

## Checking an install: `garage doctor`

```sh
garage doctor
```

Read-only, and safe to run at any time from anywhere, including a TTY after a
login that did not come up. It prints one line per check and exits 1 if anything
is actually wrong: the Hyprland version against the support floor, the key
packages, the bundled font families (Plus Jakarta Sans, Geist Mono) as fontconfig
sees them, every path this checkout
manages resolving from `$HOME` into it, dangling links left by a deleted file,
the per-user units, the plugin ABI, the generated Lua fragments (each run through
`luac -p`), and the preferences file.

Three things are reported as `note` rather than a failure, because they are true
and not problems: a machine with no plugins deployed, a home that has never
rendered, and a shell with no compositor to reach. A health check that fails on a
TTY is a health check nobody runs.

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
- **The GPU verdict is a heuristic.** It reads PCI vendor ids and device names:
  NVIDIA is always discrete, Intel is integrated unless it names itself Arc, and
  an AMD device is discrete when it carries a card model number or a known GPU
  family name. A new AMD part that breaks that naming pattern would be
  misclassified — which costs one setting either way, not a broken install. A
  virtual adapter (virtio, QXL) counts as integrated, deliberately: software
  rendering is the last place to run a full-framebuffer blur.
- **The bootstrap writes `preferences.toml` directly for the GPU gate.** Every
  other writer of that file is `garage` itself, which is how it should be. The
  CLI has no way to write a preference without also pushing it into a running
  compositor, and there is no compositor during the bootstrap — so this one
  write goes around it, keeps to two keys, and is marked in the source for the
  render/apply split to remove.
- **The plugin ABI hook watches `hyprland` only.** A machine switched to
  `hyprland-git` is on its own; the hook will not fire for it.
- **A forced install on a used system is not reversible.** Managed paths are
  backed up, but packages and enabled services are not rolled back.
- **`--dry-run` reports package names against the current sync database.** On a
  never-synced system the name check is skipped with a warning, since there is
  nothing to check against yet.
- **Re-running is intended to be safe** — the installs are `--needed`, the link
  step is `--restow`, and the generated files are only written when absent — but
  it has not been exercised from every possible starting state.
- **`garage update` cannot pull yet.** With no upstream configured it says so and
  goes on to converge the machine on whatever the local checkout holds, which is
  the only thing it can honestly do until the repositories are published.

If a `stow` conflict stops the run, each line names a path that already exists
and is not a Garage link. Move it aside and re-run `./bootstrap.sh`. More
generally: everything Garage manages under `~/.config` and `~/.local` is a
symlink into the repository, not a copy. If you delete one by mistake, re-running
`stow --restow --no-folding desktop` from the repository root puts it back
without touching anything else.
