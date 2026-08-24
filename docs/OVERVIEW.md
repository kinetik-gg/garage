# Overview

Garage is a layer on top of a normal Arch + Hyprland install, not a
replacement for either. This is the mental model, short:

## The layers

```
Hyprland (upstream compositor)
    ^
    |  renders the material
Glass (Hyprland plugin)
    ^
    |  installed, configured, versioned by
Garage
    |
    +-- Lifecycle: bootstrap.sh, updates, migrations
    +-- Settings: one schema, host preferences, generated state
    +-- Quickshell: the shell (bar, launcher, control panels, hardware OSD)
    +-- Baseline configs: the static, opinionated defaults for everything
        Hyprland, Kitty, and the rest ship with
```

**Hyprland** is the compositor this all sits on top of; it's a prerequisite,
not something Garage forks or replaces.

**Glass** is a Hyprland plugin that renders a refractive glass material. It's
independently usable outside Garage — today it builds from a pinned source
checkout the same way Garage's other ABI-pinned plugin does — but Garage
installs, pins, and rebuilds it as part of the desktop.

**Garage** is everything else: the lifecycle layer that gets a bare Arch +
Hyprland box to a working desktop and keeps it current, the settings system,
the Quickshell shell, and the baseline configuration Hyprland/Kitty/etc.
read.

## The settings system: one schema, one direction

There is one schema for host-editable settings. Data flows one way through
it:

```
shipped defaults  -->  host preferences (~/.config)  -->  generated state
                                                            (consumed by
                                                             Hyprland,
                                                             Kitty, ...)
```

- **Shipped defaults** are the schema's tracked baseline — what a fresh
  install gets before any host has an opinion.
- **Host preferences** live under `~/.config` and are where a host's edits
  actually land; they layer on top of the defaults rather than replacing the
  schema.
- **Generated state** is derived, read-only output — config fragments that
  Hyprland, Kitty, and other consumers actually read at runtime.

One writer produces the generated state, many readers consume it. Nothing
downstream of the schema writes back upstream: presentation code reads
generated state, it doesn't mutate preferences, and preferences don't hand-edit
generated fragments.

## Where a change belongs

Placement follows what kind of thing is changing:

| Kind of change | Lives in |
| --- | --- |
| Pixels drawn by the compositor | Glass |
| A value or a decision (what the value *should* be) | The settings schema |
| How something is presented on screen | Quickshell (the shell) |
| A discoverable component in the bar | A Quickshell extension manifest and, for inline content, its `Widget.qml` |
| A static, non-configurable opinion | A baseline config |
| Something that happens at install or update time | The lifecycle layer |

If a change is about what happens when the material renders, it's Glass. If
it's about what a setting's value is or how it's decided, it's the schema. If
it's about how that value is shown to the user, it's the shell. An independently
discoverable bar component belongs under the shell's `extensions/` tree; the
backend stores only its opaque composition id. If it's an opinion that isn't
meant to be a setting at all, it's a baseline config. If
it's about getting the machine into a new state — install, upgrade, migrate —
it's lifecycle.

See [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) for how the settings system,
the keybind catalog, and the plugin lifecycle actually work underneath this
model, with citations into the code.
