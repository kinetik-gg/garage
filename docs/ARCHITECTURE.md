# Architecture

Read this before touching `desktop/.local/bin/garage`, anything under `desktop/.config/hypr/`, or
the plugin lifecycle. It maps the comments in that code, which record *why* each shape is what it
is and the bug that returns if it changes. Where this file and a docstring disagree, the docstring
wins.

## 1. The three layers, and the one-writer rule

Config lives in three layers, in the order they win (`garage`, module header, around `ROOT`):

1. **Shipped defaults** — `preferences.defaults.toml`, read-only, stow-symlinked into
   `~/.config/garage`. Every schema key, at its fresh-install value.
2. **Host preferences** — the four files directly under `ROOT` (`preferences.toml`,
   `displays.toml`, `keybindings.toml`, `workspace-blocks.toml`). The only user-owned files: each
   holds the deliberate departures from layer 1, nothing else. Back these up and the desktop comes
   back.
3. **Generated state** — under `STATE_ROOT`, machine-written and deletable. `garage render`
   rewrites every fragment from layers 1 and 2; losing the directory costs one render, no settings.

**Why split**: different lifetimes — layer 1 moves with the checkout, layer 2 must survive a
reinstall, layer 3 is a cache safe to throw away. Generated output used to sit inside layer 2,
where deleting the cache meant deleting the user's settings beside it.

**One-writer rule**: `save_preferences()` is the only writer of `preferences.toml` on the settings
path, and it writes departures from layer 1, never the merged whole (see §3, and §8's four-file
split and snapshot pattern for the read side). Three writers sit off that path, deliberately and
each emitting a departures-only or stamp-only document: `compact_preferences_file()` (the v5
migration rewrite), `repair_reset()` (`garage repair --reset`), and `bootstrap.sh`'s first-boot GPU
glass gate, which writes only when the file does not exist — its comment block is headed
"ONE-WRITER VIOLATION, deliberate, narrow, and measured". `save_workspace_blocks()` and
`keybind_action()` are likewise the single writers of `workspace-blocks.toml` and
`keybindings.toml`. `displays.toml` has two: the
Displays pane's `display_finish()`, and `initialize_display_config()`, which seeds the file from
`display_snapshot()` on the first apply so a machine whose owner never opened the pane isn't left
on the catch-all monitor rule — it never overwrites an existing file. Both serialize on
`DISPLAY_LOCK` and share `normalize_display_layout()`/`layout_toml()`, so both write the same
shape. Renderers read layers 1 and 2 and write layer 3, never the reverse — with one exception, the
workspace-block allocator: `render_workspaces()` reaches `save_workspace_blocks()` by way of
`workspace_plan()` and `per_display_groups()`, so a pure `garage render` can write
`workspace-blocks.toml`. The allocation has to survive the render that produced it — unremembered,
it would be recomputed from the current display ordering, which is the bug §8's allocator row
exists to prevent.

## 2. The apply-mechanism table

`set` (`garage` `main()`, the `set` branch) writes layer 2, then walks `PREFERENCE_ROUTES` to move
the running session. Every consumer class gets there by one of these mechanisms:

| Consumer class | Mechanism | Example route step | Call site |
| --- | --- | --- | --- |
| Reads live, per frame (Hyprland core options) | `hyprctl eval` (no reload *needed* — options are dereferenced per frame) + fragment written for durability, and a silent `hyprctl reload` if the eval fails | `apply_border`, `apply_motion` | `eval_config()`; `apply_border()`. `apply_motion()` calls `hyprctl eval` directly rather than through `eval_config()`, per its docstring |
| Parse-time config (workspace/window rules, binds) | Fragment write + `hyprctl reload` | `apply_workspace_plan` | `apply_workspace_plan()` calls `render_workspaces()` then `run_or_raise(..., ["hyprctl", "reload"], ...)` |
| Signal-rereaders (other daemons) | Write file in place + `pkill -USR1`/`-USR2` | theme push | `push_theme()`: `pkill -USR2 waybar`, `pkill -USR1 kitty` |
| Startup-readers (hypridle) | Write config + `systemctl restart` | `idle` route | `PREFERENCE_ROUTES["idle"]` restarts `hypridle.service`; `render_idle()` is its `ExecStartPre` |
| inotify watchers (Quickshell) | `write_marker()`, inode preserved | accent/corner-radius/material markers | `render_accent()`, `render_corner_radius()` |
| portal-backed toolkits (GTK/libadwaita) | `gsettings set` | theme push | `push_theme()`: `gsettings set org.gnome.desktop.interface ...` |
| xsettingsd/XWayland (GTK3) | Write file + `systemctl reload-or-restart` | theme push | `push_theme()`: `reload-or-restart xsettingsd.service` |
| plugin-owned live state (glass/corner radius) | `eval_config()` first, fall back to `hyprctl reload` on error — a plugin that never loaded takes the whole eval down with it, so only the guarded fragment can apply these; `apply_glass()` raises `SettingsError` when the reload fails too | `apply_glass`, `apply_corner_radius` | `apply_glass()`; `push_corner_radius()` |

`eval_config()` exists because Hyprland 0.56+ parses config with Lua and `hyprctl keyword` refuses
to run against a non-legacy parser — `eval` is the only way to reach a live option, and it works
because options are dereferenced per frame and cached nowhere.

## 3. File-write discipline

| Writer | Mechanism | Mandatory because |
| --- | --- | --- |
| `atomic_write()` | temp file + `fsync` + `os.replace` | Default for anything a reader opens fresh each time (TOML, JSON data files). Correct, but invisible to an inotify watch on the original inode. |
| `write_lua()` | `atomic_write()` plus a `luac -p` check on a candidate file *before* installing it | Anything `hyprland.lua` `dofile()`s needs it: Hyprland's pre-apply check covers `hyprland.lua` but never follows a `dofile`, so a malformed fragment is discovered only at reload — already on disk, and loaded again every later session. |
| `write_marker()` | truncate-and-write *in place*, no rename | Any file Quickshell watches via inotify needs it: an atomic rename replaces the inode, and a watch on the original inode never sees the replacement, so it just stops firing. `SCHEME_FILE`, `ACCENT_FILE`, `CORNER_RADIUS_FILE`, `MATERIAL_FILE` go through this. |

The keybind catalog (`config/binds.lua`, §5) independently reimplements the `atomic_write()` half
in Lua, for the same reason: the reader must never see a leading fragment of the file.

## 4. Render vs apply, and the deadlock

Every subsystem splits into a **render** half (pure, writes files, signals nothing) and an
**apply**/**push** half (writes files if needed, then moves the live session) —
`render_theme()`/`push_theme()`/`apply_theme()` and
`render_accent()`/`push_accent()`/`apply_accent()` are the clearest pairs.

**The deadlock this prevents**: `set lock.*` holds `PREFERENCES_LOCK` across a *synchronous*
`systemctl restart hypridle.service`. hypridle's `ExecStartPre` re-enters this binary as
`garage render-idle`, which calls `render_idle()`. If `render_idle()` took `PREFERENCES_LOCK` it
would block forever on a lock its own caller holds while waiting for that restart to finish. Hence
`render_idle()`'s docstring: "Nothing here takes PREFERENCES_LOCK, and nothing here may ever be
made to". Same reasoning makes `compact_preferences_file()` take the lock `LOCK_NB` and skip the
rewrite if held: the migration-on-load path may run under a writer already holding it. Rule of
thumb: **render must never take the preferences lock**, because render is what a lock-holder's own
restart re-enters.

## 5. The keybind catalog contract

`config/binds.lua` is the single source of truth for the default bind set; `hyprctl binds -j`
can't reconstruct it (see `read_keybind_catalog()`'s docstring): every Lua-dispatched bind reports
as opaque `__lua`, and `displayKey` isn't serialized. So `binds.lua` publishes a TSV catalog
(`KEYBINDS_CATALOG`), one row per bind, ending in a **witness line** `#end\tN` where N is the row
count above it (`config/binds.lua:511`-`519`). `read_keybind_catalog()` treats the catalog as
unverified — readable, but nothing may conclude a bind is *absent* from it — unless that witness's
count matches what was parsed. Fail-closed against a reader catching the catalog mid-rewrite:
without the witness, a torn write reads back as a complete-but-shorter catalog with nothing
marking it a fragment.

**Rescue binds are structural, not a check.** `RESCUE` in `binds.lua` (`super+return`,
`super+space`) is consulted by `bind()` before any override is applied — a rescue bind's key never
looks at the overrides table (`config/binds.lua:204`). `guard_keybinds()` is the second lock on
the same door: it refuses any override set that leaves zero rescue binds, has no catalog, or
collides two binds onto one combination — so System Preferences can say why *before* installing,
not after the user presses the key.

## 6. Plugin lifecycle

Plugins are ABI-locked to the exact Hyprland commit + library versions they were built against.
`hyprland.lua`'s `load_plugin()` (`hyprland.lua:10`) wraps `hl.plugin.load` in `pcall`: a
stale-ABI `.so` throwing mid-`dlopen` must not abort the whole chunk and take
`binds`/`autostart`/window rules with it. Every consumer already degrades on `GLASS_AVAILABLE` /
`HYPREXPO_AVAILABLE` being false. Failures collect into `fragment_errors` and surface once at the
end of the chunk via `error(...)` into `hyprctl configerrors` (`hyprland.lua:90`) — on screen
instead of a silently missing setting. `load_override()` (`hyprland.lua:67`) applies the identical
pattern to the two generated fragments: `dofile` on a bad fragment must not take window rules and
workspace assignments with it.

`kinetik-plugin-hook`'s design-A rationale (`system/bin/kinetik-plugin-hook` header) is the same
instinct at system level: on a Hyprland ABI bump the hook **removes**, rather than rebuilds, a
stale plugin symlink. Design B — a root-context rebuild inside the pacman transaction — was
rejected on three grounds that don't expire: a pacman hook must be fast and must never fail the
transaction, and multi-minute C++ compiles don't fit; Glass's only source is the user's own
working tree, so a root build from a user-writable CMake project is a local privilege escalation;
and mid-transaction is precisely where the network may be unavailable. **Remove, don't leave
stale**: a stale symlink that still resolves would let `pcall` catch a *real* load failure from
mismatched struct layouts running global constructors before any version check can reject them — a
compositor that doesn't start, worse than one that starts without glass. Absent is a clean, silent
`io.open()` miss; mismatched is a crash.

## 7. The schema table: what adding a setting touches now

`PREFERENCE_SCHEMA` is the single Python-side source. Its header documents what a new setting used
to cost — seven touch points across three languages, two of the five Python-side ones failing
*silently* when forgotten. Seven keys shipped with no validation branch at all, and the table
carries them as the `unchecked` kind with the reason each is safe: `night_shift_enabled`, the two
wallpaper paths, `natural_scroll`, and the three touchpad switches. Adding a setting now costs:

1. **One entry in `PREFERENCE_SCHEMA`** — section/name, default, kind and its constraints (with
   the reason), and the `route` `set` walks.
2. **One line in `preferences.defaults.toml`** — deliberately not generated from the table;
   `tests/test_schema.py` is the sync check, so a mismatch is a test failure, not a silent
   disagreement.
3. **A renderer, only if no existing `render_*` already writes that surface** — most settings land
   in `preferences.lua`, which `render_preferences()` already emits.
4. **The QML control.**

`FALLBACK_DEFAULTS`, the keys `set` accepts, all of `validate_preferences()`, and
`apply_changed_preference()`'s routing are *derived* from the table, never hand-maintained —
`tests/test_schema_table.py` checks that every named routing function exists and takes the
arguments the table gives it.

## 8. Do not touch (without reading the docstring first)

| Invariant | What comes back if it changes |
| --- | --- |
| **The four-file host-preferences split** (`preferences.toml`, `displays.toml`, `keybindings.toml`, `workspace-blocks.toml`; `PREFERENCES_PATH`/`DISPLAYS_PATH`/`KEYBINDINGS_PATH`/`WORKSPACE_BLOCKS_PATH`) | `workspace-blocks.toml` is *not* folded into `displays.toml` even though it's per-display: `displays.toml` is rewritten from whatever monitors Hyprland can currently see, and a sleeping display would lose its block on that rewrite. |
| **`write_lua()`'s `luac` preflight** | Skipping it moves the failure from "caught before install" to "discovered at next reload, with the bad fragment already on disk for every future session too." |
| **`write_marker()`'s in-place truncate** | Swapping it for `atomic_write()` silently breaks every Quickshell inotify watch on that path — no error, just a UI that stops updating. |
| **The workspace block allocator** (`workspace_blocks()`) | Blocks are remembered by connector and never reclaimed, on purpose: deriving a block from a display's position in the ordering means unplugging display 2 of 3 slides display 3 into block 2 and drags its windows with it. Packing ranges tighter has the identical bug one layer down (`per_display_groups()`): Hyprland workspace ids are global, so renumbering a range relocates whatever windows live on it. |
| **The display watchdog** (`display_test()`/`display_finish()`, `_display-watchdog` in `main()`) | A layout test applies immediately but isn't written to `displays.toml` until confirmed; an unconfirmed test self-reverts after 15 seconds via a detached `_display-watchdog` subprocess, so a layout that leaves no working input device doesn't strand the user in it. |
| **The snapshot pattern** (`make_snapshot()`, and the `*_snapshot()` functions it calls) | Each is assembled fresh on every call and degrades to a fallback rather than raising — a `settings` load failure returns `FALLBACK_DEFAULTS` with an `error` string rather than taking down every other panel's data alongside it. Read-only but for one: `workspaces_snapshot()` reaches `per_display_groups()` → `workspace_blocks()` → `save_workspace_blocks()`, so a plain `garage snapshot` writes `workspace-blocks.toml` when it meets a connector for the first time. There is one allocator, and reading it means running it. |
| **`validate_preferences()`'s leniency policy** | Every bad value is coerced to its shipped default and reported, never rejected. A single raise here used to take the whole product down — every render failed, and the one screen that could have fixed the bad value (`set` loads before it writes) went read-only too. |
| **`MIMEAPPS_OVERRIDE`** | Deliberately not `~/.config/mimeapps.list`: that path is a stow symlink, and every writer of it (including `xdg-mime`) renames a temp file over it, which replaces the symlink with a plain file and cuts it loose from the repo. The desktop-prefixed override file wins by XDG spec precedence while the tracked file stays exactly as checked in. |
