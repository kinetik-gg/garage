# Garage

[![CI](https://github.com/kinetik-gg/garage/actions/workflows/ci.yml/badge.svg)](https://github.com/kinetik-gg/garage/actions/workflows/ci.yml)

Garage is an opinionated Arch + Hyprland desktop from
[Kinetik](https://github.com/kinetik-gg). The promise is simple: install minimal
Arch, run Garage's bootstrap from the TTY, reboot into a fully set up
workstation with minimal tinkering afterward.

Where dotfiles frameworks hand you a menu of choices to assemble yourself,
Garage ships the decisions already made — one settings schema, one shell, one
baseline configuration per tool — and gives you a lifecycle (bootstrap,
update, migrate) to keep that opinionated setup current instead of drifting.
Its sibling project, [Glass](https://github.com/kinetik-gg/glass), is a
Hyprland plugin that renders a refractive glass material; it's independently
usable, and Garage pins and deploys it as an optional part of the desktop.

Garage is **not**:

- A dotfiles framework with a choice matrix to configure — the opinions are
  the product.
- A Linux distribution or ISO — installing Arch stays your job.
- A Hyprland fork.
- A plugin suite (Glass is its own project; Garage consumes it).

<!-- Screenshot placeholder: a shot of the finished desktop belongs here. -->

## Install

Prerequisites: a freshly installed **minimal Arch system with no desktop
environment**, a network connection, and a normal user with `sudo`. You do not
need Hyprland, a display manager, or any graphical software — Garage installs
those. The bootstrap runs from a bare TTY login.

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

## Core setup

- Hyprland compositor
- Kitty terminal
- Fish shell with the Pure prompt
- Thunar file explorer, with Garage chrome, split view, archive/media actions,
  recursive search, and background thumbnailing
- Rofi launcher
- Quickshell shell: the status bar, launcher, notification center, control
  panels, and settings UI — the bar runs inside the shell, not as a separate
  program
- SwayOSD volume and brightness feedback
- A Rust backend behind it all: the `garage` CLI plus the `garage-metrics`,
  `garage-file-index`, `garage-ai-usage`, and `garage-bar-probe` helpers, built
  from `backend/` at install time into `~/.local/lib/garage/bin`
- ABI-pinned Hyprland plugins (Glass and `hyprexpo`)
- Spotify, Discord, and Zed
- Docker with Compose and Buildx (you are *not* added to the `docker` group —
  see [`docs/INSTALL.md`](docs/INSTALL.md))
- Node.js/TypeScript, Rust, and C/C++ development toolchains

## Layout

See [`docs/OVERVIEW.md`](docs/OVERVIEW.md) for the mental model: how Hyprland,
Glass, the settings schema, the Quickshell shell, and the baseline configs
relate, and where a given change belongs. See
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) before changing the settings
backend, the keybind catalog, or the plugin lifecycle — it's a map to the
comments that already explain why each is shaped the way it is.

See [`docs/LAUNCHER.md`](docs/LAUNCHER.md) for launcher queries, commands, and
their activation behavior; [`docs/AUTHENTICATION.md`](docs/AUTHENTICATION.md)
for Garage's scoped polkit modal and the checksum-pinned upstream backend it
retains; and [`docs/SESSION-SURFACES.md`](docs/SESSION-SURFACES.md) for SDDM
and Hyprlock ownership, monitor routing, and safe test procedures.

## Hyprland plugins

Both plugins are optional: Hyprland's configuration guards each load, so the
desktop comes up without them. The bootstrap deploys them when a Glass source
checkout is present and warns and skips otherwise — you lose the glass
material and `hyprexpo`, nothing else.

Glass is developed in place at `~/repositories/glass`; the deploy script only
ever reads it and refuses to run if its tree is dirty or sits on a commit other
than the pin. `hyprexpo` is a disposable pinned checkout fetched into
`~/.cache/hyprland-plugin-src`. Both revisions are pinned in
`system/plugin-pins` and deployed to immutable directories named after
Hyprland's ABI. After a Hyprland update, run:

```sh
~/.config/hypr/scripts/garage-rebuild-plugins
```

Log out and back in after deploying a build for a new ABI.

## Staying current

```sh
garage update            # pull, relink, converge, reload
garage update --dry-run  # print all of that and do none of it
garage doctor            # read-only health check, one line per problem
```

`garage update` is the whole lifecycle in one command: it fast-forwards the
checkout, backs up your preference files to
`~/.garage-backup/<timestamp>/pre-update/`, sweeps links to files the new
version deleted, re-runs the bootstrap (safe on an existing install — see
[`docs/INSTALL.md`](docs/INSTALL.md)), then renders, runs any schema migration
the new version added, and reloads Hyprland. `garage doctor` is safe anywhere,
including a TTY after a login that did not come up.

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

## Status

Garage is young and moves fast. It is built for Kinetik's own machines first
and generalized second: expect opinionated defaults, occasional sharp edges,
and a lifecycle that assumes you update rather than fork. `garage doctor
--report` output makes a good bug report.

## License

MIT — see [`LICENSE`](LICENSE).
