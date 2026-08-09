# Garage

Garage is an opinionated Arch + Hyprland desktop layer from
[Kinetik](https://github.com/kinetik-gg). The promise is simple: install a
barebone Arch + Hyprland system, run Garage's bootstrap, and end up with a
fully set up workstation with minimal tinkering afterward.

Where dotfiles frameworks hand you a menu of choices to assemble yourself,
Garage ships the decisions already made — one settings schema, one shell, one
baseline configuration per tool — and gives you a lifecycle (bootstrap,
update, migrate) to keep that opinionated setup current instead of drifting.
Its sibling project, [Glass](https://github.com/kinetik-gg), is a Hyprland
plugin that renders a refractive glass material; it's independently usable,
and Garage installs and pins it as part of the desktop.

Garage is **not**:

- A dotfiles framework with a choice matrix to configure — the opinions are
  the product.
- A Linux distribution or ISO — a working Arch + Hyprland install stays a
  prerequisite.
- A Hyprland fork.
- A plugin suite (Glass is its own project; Garage consumes it).

## Install

On a bootable vanilla Arch base with Hyprland as a target and a normal user
with `sudo`:

```sh
git clone https://github.com/kinetik-gg/garage.git ~/repositories/garage
cd ~/repositories/garage
./bootstrap.sh
```

See [`docs/INSTALL.md`](docs/INSTALL.md) for prerequisites, what bootstrap
does today, and known gaps in the installer.

For config-only deployment, run
`stow --target="$HOME" --no-folding desktop`. Using `--no-folding` keeps runtime
files created by tools such as Fisher outside the Git repository. Remove the
managed links with `stow --target="$HOME" --delete desktop`.

## Layout

See [`docs/OVERVIEW.md`](docs/OVERVIEW.md) for the mental model: how Hyprland,
Glass, the settings schema, the Quickshell shell, and the baseline configs
relate, and where a given change belongs.

## Core setup

- Hyprland compositor
- Kitty terminal
- Fish shell with the Pure prompt
- Rofi launcher
- Waybar status bar
- Quickshell shell (settings UI, launcher, control panels)
- SwayNC Control Center and notifications
- SwayOSD volume and brightness feedback
- ABI-pinned Hyprland plugins (Glass and `hyprexpo`)
- Spotify, Discord, and Zed
- Docker with Compose and Buildx
- Node.js/TypeScript, Rust, and C/C++ development toolchains

## Hyprland plugins

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

## License

MIT — see [`LICENSE`](LICENSE).
