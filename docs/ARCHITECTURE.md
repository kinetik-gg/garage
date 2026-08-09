# Architecture

Read this before touching `desktop/.local/bin/garage`, any file under
`desktop/.config/hypr/`, or the plugin lifecycle. It distills the comments
already in that code — they record *why* each shape is what it is, and the
bug that would come back if it changed. This document is a map to them, not
a replacement; when a section here and a docstring disagree, the docstring
is right and this file is stale.

## 1. The three layers, and the one-writer rule

Config lives in three layers, in the order they win (`garage`, module header,
around `ROOT`):

1. **Shipped defaults** — `preferences.defaults.toml`, read-only, reaching
   `~/.config/garage` as a stow symlink. Every schema key, at the value a
   fresh install starts on.
2. **Host preferences** — the four files directly under `ROOT`
   (`preferences.toml`, `displays.toml`, `keybindings.toml`,
   `workspace-blocks.toml`). The only user-owned files on the machine: each
   holds the deliberate departures from layer 1 and nothing else. Back these
   up and the desktop comes back.
3. **Generated state** — under `STATE_ROOT`, machine-written and deletable.
   Every fragment under it is rewritten from layers 1 and 2 by `garage
   render`; losing the whole directory costs one render, no settings.

They are split because they have different lifetimes: layer 1 moves with the
checkout, layer 2 must survive a reinstall, layer 3 is a cache safe to throw
away. Generated output used to sit inside layer 2, where deleting the cache
meant deleting the user's settings beside it — this is the bug the split
exists to prevent.

**One-writer rule**: `save_preferences()` (`garage:1236`) is the only writer
of `preferences.toml`; it always writes departures from layer 1, never the
merged whole (see §3, §8's four-file split, and §8's snapshot pattern for the
read side of the same discipline). Each of the other three host files has
exactly one writer too: `save_workspace_blocks()` (`garage:1925`) for
`workspace-blocks.toml`, the Displays pane's `display_finish()`
(`garage:4216`) for `displays.toml`, and `keybind_action()` (`garage:2627`)
for `keybindings.toml`. Nothing downstream of the schema writes back
upstream: renderers read generated state, they don't mutate preferences.

## 2. The apply-mechanism table

`set` (`garage` `main()`, the `set` branch) writes layer 2, then walks
`PREFERENCE_ROUTES` (`garage:739`) to move the running session. Every
consumer class gets there by one of these mechanisms — verified against a
real call site each:

| Consumer class | Mechanism | Example route step | Call site |
| --- | --- | --- | --- |
| Reads live, per frame (Hyprland core options) | `hyprctl eval` (no reload) + fragment written for durability | `apply_border`, `apply_motion` | `eval_config()` `garage:3629`; `apply_border()` `garage:3705` |
| Parse-time config (workspace/window rules, binds) | Fragment write + `hyprctl reload` | `apply_workspace_plan` | `apply_workspace_plan()` `garage:3796` calls `render_workspaces()` then `run_or_raise(..., ["hyprctl", "reload"], ...)` |
| Signal-rereaders (other daemons) | Write file in place + `pkill -USR1`/`-USR2` | theme push | `push_theme()` `garage:3576`: `pkill -USR2 waybar`, `pkill -USR1 kitty` |
| Startup-readers (hypridle) | Write config + `systemctl restart` | `idle` route | `PREFERENCE_ROUTES["idle"]` `garage:766` restarts `hypridle.service`; `render_idle()` `garage:1694` is its `ExecStartPre` |
| inotify watchers (Quickshell) | `write_marker()`, inode preserved | accent/corner-radius/material markers | `render_accent()` `garage:1741`, `render_corner_radius()` `garage:3676` |
| portal-backed toolkits (GTK/libadwaita) | `gsettings set` | theme push | `push_theme()` `garage:3576`: `gsettings set org.gnome.desktop.interface ...` |
| xsettingsd/XWayland (GTK3) | Write file + `systemctl reload-or-restart` | theme push | `push_theme()` `garage:3576`: `reload-or-restart xsettingsd.service` |
| plugin-owned live state (glass/corner radius) | `eval_config()` first, fall back to `hyprctl reload` on error | `apply_glass`, `apply_corner_radius` | `apply_glass()` `garage:3654`; `push_corner_radius()` `garage:3682` |

`eval_config()` exists because Hyprland 0.56+ parses config with Lua and
`hyprctl keyword` refuses to run against a non-legacy parser — `eval` is the
only way to reach a live option, and it works because options are
dereferenced per frame and cached nowhere (`garage:3629`).

## 3. File-write discipline

Three writers, each mandatory for a different reason:

- **`atomic_write()`** (`garage:1220`) — temp file + `fsync` + `os.replace`.
  Default for anything a reader opens fresh each time (TOML files, JSON data
  files). Correct, but invisible to an inotify watch on the original inode.
- **`write_lua()`** (`garage:1254`) — `atomic_write()` plus a `luac -p`
  syntax check against a candidate file *before* installing it. Mandatory
  for anything `hyprland.lua` `dofile()`s: Hyprland's own pre-apply check
  covers `hyprland.lua` but never follows a `dofile`, so a malformed
  fragment would otherwise only be discovered at reload — by which point
  it's already on disk and every later session loads it again.
- **`write_marker()`** (`garage:1287`) — truncate-and-write *in place*, no
  rename. Mandatory for any file Quickshell watches via inotify: an atomic
  rename replaces the inode, and a watch registered on the original inode
  never sees the replacement come and go — the watcher would just stop
  firing. `SCHEME_FILE`, `ACCENT_FILE`, `CORNER_RADIUS_FILE`, `MATERIAL_FILE`
  all go through this.

The keybind catalog (`config/binds.lua`, §5) independently reimplements the
`atomic_write()` half in Lua, for the same reason: the reader must never see
a leading fragment of the file.

## 4. Render vs apply, and the deadlock

Every subsystem splits into a **render** half (pure, writes files, signals
nothing) and an **apply**/**push** half (writes files if needed, then moves
the live session) — see `render_theme()`/`push_theme()`/`apply_theme()`
(`garage:3554`-`3627`) and `render_accent()`/`push_accent()`/`apply_accent()`
(`garage:1741`-`1757`) as the clearest pairs.

**The deadlock this prevents**: `set lock.*` holds `PREFERENCES_LOCK` across
a *synchronous* `systemctl restart hypridle.service`. hypridle's
`ExecStartPre` re-enters this binary as `garage render-idle`, which calls
`render_idle()` (`garage:1694`). If `render_idle()` ever tried to acquire
`PREFERENCES_LOCK`, it would block forever waiting on a lock its own caller
is holding while waiting for it to finish — a restart that can never
complete. The docstring is explicit: "Nothing here takes PREFERENCES_LOCK,
and nothing here may ever be made to" (`garage:1705`). The same reasoning
gates `compact_preferences_file()`'s lock acquisition non-blocking
(`garage:1004`-`1012`): the migration-on-load path may run under a writer
that already holds the lock, so it takes the lock `LOCK_NB` and simply skips
the rewrite if held, rather than risk the same re-entrant deadlock.

Rule of thumb: **render must never take the preferences lock**, because
render is what a lock-holder's own restart re-enters.

## 5. The keybind catalog contract

`config/binds.lua` is the single source of truth for the default bind set —
`hyprctl binds -j` can't reconstruct it (see `read_keybind_catalog()`
docstring, `garage:2473`): every Lua-dispatched bind reports as opaque
`__lua`, and `displayKey` isn't serialized either. So `binds.lua` publishes
its own catalog as a TSV (`KEYBINDS_CATALOG`), one row per bind, ending in a
**witness line**: `#end\tN` where N is the row count above it
(`config/binds.lua:511`-`519`). `read_keybind_catalog()` treats the catalog
as unverified — readable, but nothing may conclude a bind is *absent* from
it — unless the file ends in a witness whose count matches what was actually
parsed. This is fail-closed against a reader catching the catalog
mid-rewrite: without the witness, a torn/truncated write reads back as a
complete-but-shorter catalog and nothing marks it as a fragment.

**Rescue binds are structural, not a check.** `RESCUE` in `binds.lua`
(`super+return`, `super+space`) is consulted by `bind()` before an override
is ever applied — a rescue bind's key literally never looks at the overrides
table (`config/binds.lua:204`). `guard_keybinds()` (`garage:2536`) is the
second lock on the same door: it refuses to install any override set that
would leave zero rescue binds, has no catalog, or collides two binds onto
one combination — positioned so System Preferences can say why *before*
installing, rather than the user finding out by pressing the key.

## 6. Plugin lifecycle

Plugins are ABI-locked to the exact Hyprland commit + library versions they
were built against. `hyprland.lua`'s `load_plugin()` (`hyprland.lua:10`)
wraps `hl.plugin.load` in `pcall`: a stale-ABI `.so` throwing mid-`dlopen`
must not abort the whole chunk and take `binds`/`autostart`/window rules down
with it. Every consumer already degrades on `GLASS_AVAILABLE` /
`HYPREXPO_AVAILABLE` being false. Failures collect into `fragment_errors` and
surface once, at the end of the chunk, via `error(...)` into `hyprctl
configerrors` (`hyprland.lua:90`) — visible on screen instead of a silently
missing setting. `load_override()` for the two generated fragments
(`hyprland.lua:67`) uses the identical pattern for the identical reason:
`dofile` on a bad fragment must not take window rules and workspace
assignments with it.

`kinetik-plugin-hook`'s design-A rationale (`system/bin/kinetik-plugin-hook`
header) is the same "never take the desktop down" instinct at the system
level: on a Hyprland ABI bump, the hook **removes**, rather than rebuilds,
a stale plugin symlink. Design B — a root-context rebuild inside the pacman
transaction — was rejected on three grounds that don't expire: a pacman hook
must be fast and must never fail the transaction, and multi-minute C++
compiles don't fit; Glass's only source is the user's own working tree, and a
root build from a user-writable CMake project is a local privilege
escalation; and a hook running mid-transaction is precisely where the
network may be unavailable. **Remove, don't leave stale**: a stale symlink
that still resolves would let `pcall` catch a *real* load failure from
mismatched struct layouts running global constructors before any version
check can reject them — a compositor that doesn't start, which is worse than
one that starts without glass. Absent is a clean, silent `io.open()` miss;
mismatched is a crash.

## 7. The schema table: what adding a setting touches now

`PREFERENCE_SCHEMA` (`garage:412`) is the single Python-side source. Its
header (`garage:332`-`404`) documents what a new setting used to cost — seven
touch points across three languages, with two of five Python-side ones
failing *silently* when forgotten (`night_shift_enabled` and four others
shipped for releases with no validation branch at all). Adding a setting now
costs:

1. **One entry in `PREFERENCE_SCHEMA`** — section/name, default, kind and
   its constraints (with the reason), and the `route` `set` walks.
2. **One line in `preferences.defaults.toml`** — deliberately not generated
   from the table; `tests/test_schema.py` is the sync check, so a mismatch
   is a test failure, not a silent disagreement.
3. **A renderer, only if no existing `render_*` already writes that
   surface** — most settings land in `preferences.lua`, which
   `render_preferences()` already emits.
4. **The QML control.**

`FALLBACK_DEFAULTS`, the keys `set` accepts, all of `validate_preferences()`,
and `apply_changed_preference()`'s routing are all *derived* from the table
and never hand-maintained separately — `tests/test_schema_table.py` checks
that every named routing function actually exists and takes the arguments
the table gives it.

## 8. Do not touch (without reading the docstring first)

- **The four-file host-preferences split** (`preferences.toml`,
  `displays.toml`, `keybindings.toml`, `workspace-blocks.toml`, `garage:34`).
  `workspace-blocks.toml` in particular is *not* folded into `displays.toml`
  even though it's per-display: `displays.toml` is rewritten from whatever
  monitors Hyprland can currently see, and a sleeping display would lose its
  block on that rewrite.
- **`write_lua()`'s `luac` preflight** (`garage:1254`). Skipping it moves the
  failure from "caught before install" to "discovered at next reload, with
  the bad fragment already on disk for every future session too."
- **`write_marker()`'s in-place truncate** (`garage:1287`). Swapping it for
  `atomic_write()` silently breaks every Quickshell inotify watch on that
  path — no error, just a UI that stops updating.
- **The workspace block allocator** (`workspace_blocks()`, `garage:1934`).
  Blocks are remembered by connector and never reclaimed, on purpose:
  deriving a block from a display's position in the ordering means
  unplugging display 2 of 3 slides display 3 into block 2 and drags its
  windows with it. Packing ranges tighter has the identical bug one layer
  down (`per_display_groups()`, `garage:1968`): Hyprland workspace ids are
  global, so renumbering any range relocates whatever windows are living on
  it.
- **The display watchdog** (`display_test()`/`display_finish()`,
  `garage:4199`-`4231`, `_display-watchdog` in `main()`). A layout test
  applies immediately but isn't written to `displays.toml` until confirmed;
  an unconfirmed test self-reverts after 15 seconds via a detached
  `_display-watchdog` subprocess, so a layout that leaves no working input
  device doesn't strand the user in it.
- **The snapshot pattern** (`make_snapshot()`, `garage:4031`, and the
  `*_snapshot()` functions it calls). Each is read-only, assembled fresh on
  every call, and degrades to a fallback rather than raising — a `settings`
  load failure returns `FALLBACK_DEFAULTS` with an `error` string rather
  than taking down every other panel's data alongside it.
- **`validate_preferences()`'s leniency policy** (`garage:1328`). Every bad
  value is coerced to its shipped default and reported, never rejected. A
  single raise here used to take the whole product down — every render
  failed, and the one screen that could have fixed the bad value (`set`
  loads before it writes) went read-only too.
- **`MIMEAPPS_OVERRIDE`** (`garage:100`). Deliberately not
  `~/.config/mimeapps.list`: that path is a stow symlink, and every writer of
  it (including `xdg-mime`) renames a temp file over it, which replaces the
  symlink with a plain file and cuts it loose from the repo. The
  desktop-prefixed override file wins by XDG spec precedence while the
  tracked file stays exactly as checked in.
