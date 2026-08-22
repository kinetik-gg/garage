# Garage

Garage is an opinionated Arch + Hyprland desktop from
[Kinetik](https://github.com/kinetik-gg). The promise is simple: install minimal
Arch, run Garage's bootstrap from the TTY, reboot into a fully set up
workstation with minimal tinkering afterward.

Where dotfiles frameworks hand you a menu of choices to assemble yourself,
Garage ships the decisions already made — one settings schema, one shell, one
baseline configuration per tool — and gives you a lifecycle (bootstrap,
update, migrate) to keep that opinionated setup current instead of drifting.
Its sibling project, [Glass](https://github.com/kinetik-gg), is a Hyprland
plugin that renders a refractive glass material; it's independently usable, and
Garage pins and deploys it as an optional part of the desktop when its source is
available.

Garage is **not**:

- A dotfiles framework with a choice matrix to configure — the opinions are
  the product.
- A Linux distribution or ISO — installing Arch stays your job.
- A Hyprland fork.
- A plugin suite (Glass is its own project; Garage consumes it).

## Install

Prerequisites: a freshly installed **minimal Arch system with no desktop
environment**, a network connection, and a normal user with `sudo`. You do not
need Hyprland, a display manager, or any graphical software — Garage installs
those. The bootstrap runs from a bare TTY login.

```sh
sh -c "$(curl -fsSL https://get.kinetik.gg/garage)"
```

**The one-liner is not live yet** — `get.kinetik.gg` is a placeholder while
Garage's repositories are local-only. The clone path below is equally supported
and is exactly what the one-liner does for you:

```sh
git clone https://github.com/kinetik-gg/garage.git ~/repositories/garage
cd ~/repositories/garage
./bootstrap.sh
```

Then reboot and pick **Hyprland (UWSM)** at the login screen.

`./bootstrap.sh --dry-run` prints every change it would make and makes none of
them.

**Garage expects a fresh system; it will refuse a used one.** An enabled display
manager, another desktop's session files, a populated `~/.config`, or an AUR
helper on `PATH` each stop the installer, because Garage claims `~/.config`
wholesale and installs its own display manager, session, and notification
daemon. `GARAGE_FORCE=1 ./bootstrap.sh` overrides the check; displaced files go
to `~/.garage-backup/<timestamp>/`.

See [`docs/INSTALL.md`](docs/INSTALL.md) for the full prerequisites, what
bootstrap does step by step, the Docker group note, and the known gaps.

For config-only deployment, run
`stow --target="$HOME" --no-folding desktop`. Using `--no-folding` keeps runtime
files created by tools such as Fisher outside the Git repository. Remove the
managed links with `stow --target="$HOME" --delete desktop`.

## Layout

See [`docs/OVERVIEW.md`](docs/OVERVIEW.md) for the mental model: how Hyprland,
Glass, the settings schema, the Quickshell shell, and the baseline configs
relate, and where a given change belongs. See
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) before changing the settings
backend, the keybind catalog, or the plugin lifecycle — it's a map to the
comments that already explain why each is shaped the way it is.

See [`docs/LAUNCHER.md`](docs/LAUNCHER.md) for launcher queries, commands, and
their activation behavior.

See [`docs/AUTHENTICATION.md`](docs/AUTHENTICATION.md) for Garage's scoped
polkit modal and the checksum-pinned upstream backend it retains.

## Core setup

- Hyprland compositor
- Kitty terminal
- Fish shell with the Pure prompt
- Thunar file explorer, with Garage chrome, split view, archive/media actions,
  recursive search, and background thumbnailing
- Rofi launcher
- Quickshell status bar (the shell's own top panel)
- Quickshell shell (settings UI, launcher, control panels)
- Native notification center and control center (Quickshell + Glass)
- SwayOSD volume and brightness feedback
- ABI-pinned Hyprland plugins (Glass and `hyprexpo`)
- Spotify, Discord, and Zed
- Docker with Compose and Buildx (you are *not* added to the `docker` group —
  see [`docs/INSTALL.md`](docs/INSTALL.md))
- Node.js/TypeScript, Rust, and C/C++ development toolchains

## Hyprland plugins

Both plugins are optional: Hyprland's configuration guards each load, so the
desktop comes up without them. Glass is not published yet, so on a fresh install
the bootstrap warns and skips the plugin deploy rather than failing.

Glass is developed in place at `~/repositories/glass` (with a transition
fallback to `~/repositories/hyprliquid`); the deploy script only ever reads it
and refuses to run if its tree is dirty or sits on a commit other than the
pin. `hyprexpo` is a disposable pinned checkout fetched into
`~/.cache/hyprland-plugin-src`. Both revisions are pinned in
`~/.config/hypr/scripts/garage-rebuild-plugins` and deployed to immutable
directories named after Hyprland's ABI. After a Hyprland update, run:

```sh
~/.config/hypr/scripts/garage-rebuild-plugins
```

Log out and back in after deploying a build for a new ABI.

## Recovery

If the desktop will not come up, switch to a TTY (**Ctrl+Alt+F3**) and log in
there — none of the commands below needs a running compositor.

```sh
garage doctor            # what is wrong, with a hint per problem
garage doctor --report   # the same checks as JSON, to paste into a bug report
garage repair --reset    # back up an unreadable preferences.toml, start it fresh
garage update            # re-converge this machine on the checkout
```

`garage repair` on its own only reports; `--reset` acts, and keeps your old file
beside the new one. Configs under `~/.config` are symlinks into the checkout, so
a damaged tracked file comes back with the restow that `garage update` runs.

## License

MIT — see [`LICENSE`](LICENSE).
