# Architecture

Read this before touching `backend/crates`, anything under `desktop/.config/hypr/`, or the plugin
lifecycle. It maps the rustdoc comments in that code, which record *why* each shape is what it is
and the bug that returns if it changes. Where this file and rustdoc disagree, rustdoc wins.

## 1. The three layers, and the one-writer rule

Config lives in three layers, in the order `Paths::from_env()` constructs them:

1. **Shipped defaults** — `preferences.defaults.toml`, read-only, stow-symlinked into
   `~/.config/garage`. Every schema key, at its fresh-install value.
2. **Host preferences** — the four files under `Paths::root` (`preferences.toml`,
   `displays.toml`, `keybindings.toml`, `workspace-blocks.toml`). The only user-owned files: each
   holds the deliberate departures from layer 1, nothing else. Back these up and the desktop comes
   back.
3. **Generated state** — under `Paths::state_root`, machine-written and deletable. `garage render`
   rewrites every fragment from layers 1 and 2; losing the directory costs one render, no settings.

**Why split**: different lifetimes — layer 1 moves with the checkout, layer 2 must survive a
reinstall, layer 3 is a cache safe to throw away. Generated output used to sit inside layer 2,
where deleting the cache meant deleting the user's settings beside it.

Wallpaper assets have their own source/production boundary. `assets/wallpapers/` owns the
originals, credits, and provenance manifest; `assets/wallpapers/build` produces the 4K JPEGs in
`desktop/Wallpaper/` that Stow publishes under `~/Wallpaper`. Source and production basenames stay
identical because preferences and `~/.local/share/wallpaper/current` persist those production
paths. Only the 4K set belongs in the stow tree: hyprpaper decodes image dimensions, independent of
the compressed file size.

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
on the catch-all monitor rule — it never overwrites an existing file. Both serialize through
`DisplayLock::acquire()` and share `normalize_display_layout()`/`layout_toml()`, so both write the same
shape. Renderers read layers 1 and 2 and write layer 3, never the reverse — with one exception, the
workspace-block allocator: `render_workspaces()` reaches `save_workspace_blocks()` by way of
`workspace_plan()` and `per_display_groups()`, so a pure `garage render` can write
`workspace-blocks.toml`. The allocation has to survive the render that produced it — unremembered,
it would be recomputed from the current display ordering, which is the bug §8's allocator row
exists to prevent.

## 2. The apply-mechanism table

`garage set` enters `garage-cli`'s `set()` handler, writes layer 2, then walks
`Route::steps()` to move the running session. Every consumer class gets there by one of these
mechanisms:

| Consumer class | Mechanism | Example route step | Call site |
| --- | --- | --- | --- |
| Reads live, per frame (Hyprland core options) | `hyprctl eval` (no reload *needed* — options are dereferenced per frame) + fragment written for durability, and a silent `hyprctl reload` if the eval fails | `ApplyStep::Border`, `ApplyStep::Motion` | `eval_config()`; `apply_border()`. `apply_motion()` calls `hyprctl eval` directly rather than through `eval_config()`, per its rustdoc |
| Parse-time config (workspace/window rules, binds) | Fragment write + `hyprctl reload` | `ApplyStep::WorkspacePlan` | `apply_workspace_plan()` calls `render_workspaces_for()` then `run_or_raise()` with `&["hyprctl", "reload"]` |
| Signal-rereaders (other daemons) | Write file in place + `pkill -USR1`/`-USR2` | `ApplyStep::ThemeIfSchemeMoved` | `push_theme()`: `pkill -USR1 kitty` |
| Startup-readers (hypridle) | Write config + `systemctl restart` | `Route::Idle` | `Route::steps()` carries the `hypridle.service` restart after `RenderStep::Idle`; `render_idle()` is its `ExecStartPre` |
| inotify watchers (Quickshell) | `write_marker()`, inode preserved | marker-writing render and apply steps | `render_accent()`, `render_corner_radius()` |
| portal-backed toolkits (GTK/libadwaita) | `gsettings set` | `ApplyStep::ThemeIfSchemeMoved` | `push_theme()`: `gsettings set org.gnome.desktop.interface ...` |
| xsettingsd/XWayland (GTK3) | Write file + `systemctl reload-or-restart` | `ApplyStep::ThemeIfSchemeMoved` | `push_theme()`: `reload-or-restart xsettingsd.service` |
| plugin-owned live state (glass/corner radius) | `eval_config()` first, fall back to `hyprctl reload` on error — a plugin that never loaded takes the whole eval down with it, so only the guarded fragment can apply these; `apply_glass()` returns `ApplyError::Settings` when the reload fails too | `ApplyStep::Glass`, `ApplyStep::CornerRadius` | `apply_glass()`; `push_corner_radius()` |

`eval_config()` exists because Hyprland 0.56+ parses config with Lua and `hyprctl keyword` refuses
to run against a non-legacy parser — `eval` is the only way to reach a live option, and it works
because options are dereferenced per frame and cached nowhere.

## 3. File-write discipline

All three writers live under `garage-core::fs`, so renderers and appliers share one
implementation of each discipline:

| Writer | Mechanism | Mandatory because |
| --- | --- | --- |
| `atomic_write()` | target-directory temporary + `fsync` + `std::fs::rename` | Default for anything a reader opens fresh each time (TOML, JSON data files). Correct, but invisible to an inotify watch on the original inode. |
| `write_lua()` | candidate file checked through `LuaSyntaxCheck`, then the same text installed with `atomic_write()` | Anything `hyprland.lua` `dofile()`s needs it: Hyprland's pre-apply check covers `hyprland.lua` but never follows a `dofile`, so a malformed fragment is discovered only at reload — already on disk, and loaded again every later session. |
| `write_marker()` | truncate-and-write *in place*, no rename | Any file Quickshell watches via inotify needs it: an atomic rename replaces the inode, and a watch on the original inode never sees the replacement, so it just stops firing. The `Paths::markers` scheme, accent, corner-radius, and material paths go through this. |

The keybind catalog (`config/binds.lua`, §5) independently reimplements the `atomic_write()` half
in Lua, for the same reason: the reader must never see a leading fragment of the file.

## 4. Render vs apply, and the deadlock

Every subsystem splits into a **render** half (pure, writes files, signals nothing) and an
**apply**/**push** half (writes files if needed, then moves the live session) —
`render_theme()`/`push_theme()`/`apply_theme()` and
`render_accent()`/`push_accent()`/`apply_accent()` are the clearest pairs.

The split is now structural: `garage-render`'s crate graph cannot name the preferences lock or
the process runner in `garage-proc`, and the `workspace_shape` cargo integration test rejects
either forbidden dependency edge, including a transitive one.

**The deadlock this prevents**: `set lock.*` holds `PrefLock::acquire()` across a *synchronous*
`systemctl restart hypridle.service`. hypridle's `ExecStartPre` starts the installed
`garage render-idle` command, which calls `render_idle()`. If `render_idle()` called
`PrefLock::acquire()` it would block forever on a lock its own caller holds while waiting for that
restart to finish. Hence `render_idle()`'s rustdoc: "Nothing here takes the preferences lock, and
nothing here may ever be made to". Same reasoning makes `compact_preferences_file()` call
`PrefLock::try_acquire()` and skip the rewrite if held: the migration-on-load path may run under a
writer already holding it. Rule of thumb: **render must never take the preferences lock**, because
render is what a lock-holder's own restart re-enters.

## 5. The keybind catalog contract

`config/binds.lua` is the single source of truth for the default bind set; `hyprctl binds -j`
can't reconstruct it (see `read_keybind_catalog()`'s rustdoc): every Lua-dispatched bind reports
as opaque `__lua`, and `displayKey` isn't serialized. So `binds.lua` publishes a TSV catalog at
`Paths::fragments.keybinds_catalog`, one row per bind, ending in a **witness line** `#end\tN` where N is the row
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

## 7. The `preferences!` declaration and `PreferenceKey`: what adding a setting touches now

The `preferences!` invocation in `garage-core/src/schema/prefs.rs` is the single Rust-side key
declaration. The macro emits the typed preference structs and `PreferenceKey`; `coerce_from()`
walks those declared keys instead of relying on a parallel validation ladder. Its header documents
what a new setting used to cost — seven touch points across three languages, two of the five
Python-side ones failing *silently* when forgotten. Seven keys shipped with no validation branch
at all, and the declaration carries them as unchecked types with the reason each is safe:
`night_shift_enabled`, the two wallpaper paths, `natural_scroll`, and the three touchpad switches.
Adding a setting now costs:

1. **One entry in the `preferences!` invocation** — variant, section/name, type and its constraints
   (with the reason), and the `Route` `set` walks.
2. **One line in `preferences.defaults.toml`** — deliberately not generated from the table;
   `defaults_file_has_every_key()` is the sync check, so a mismatch is a test failure, not a silent
   disagreement.
3. **A renderer, only if no existing `render_*` already writes that surface** — most settings land
   in `preferences.lua`, which `render_preferences()` already emits.
4. **The QML control.**

The macro generates `PreferenceKey::ALL`, the dotted-key parser `set` accepts, and the typed
default, coerce, set, access, and key-to-`Route` paths. `Route::steps()` is the separate typed
mechanism table that `apply_changed_preference()` walks; exhaustive enum dispatch and the schema
crate's unit tests keep declaration order, shipped defaults, routes, and steps in sync.

## 8. Where a theme would attach

No theme data or selection exists today: the two-scheme `PALETTE` is the whole look today. Three
dormant seams mark where a future themes-as-data implementation would attach:

- `template_search_path()` owns runtime template precedence; a selected theme directory would go
  in front so each file overrides independently and omitted templates keep inheriting.
- `role()` is the palette lookup seam and documents the ownership and context cost of loading a
  palette from disk.
- `Paths::themes` is the resolution root. Nothing writes there today.

Activating those seams first requires an `appearance.theme` schema key — rebuild-class work under
the values-are-data doctrine — a doctor row reporting the resolved theme, and re-validation of
every rendered surface. Until all three exist, no current-theme state belongs in resolution.

## 9. Do not touch (without reading the rustdoc first)

| Invariant | What comes back if it changes |
| --- | --- |
| **The four-file host-preferences split** (`preferences.toml`, `displays.toml`, `keybindings.toml`, `workspace-blocks.toml`; the four `Paths::host` fields built by `Paths::from_env()`) | `workspace-blocks.toml` is *not* folded into `displays.toml` even though it's per-display: `displays.toml` is rewritten from whatever monitors Hyprland can currently see, and a sleeping display would lose its block on that rewrite. |
| **`write_lua()`'s `luac` preflight** | Skipping it moves the failure from "caught before install" to "discovered at next reload, with the bad fragment already on disk for every future session too." |
| **`write_marker()`'s in-place truncate** | Swapping it for `atomic_write()` silently breaks every Quickshell inotify watch on that path — no error, just a UI that stops updating. |
| **The workspace block allocator** (`workspace_blocks()`) | Blocks are remembered by connector and never reclaimed, on purpose: deriving a block from a display's position in the ordering means unplugging display 2 of 3 slides display 3 into block 2 and drags its windows with it. Packing ranges tighter has the identical bug one layer down (`per_display_groups()`): Hyprland workspace ids are global, so renumbering a range relocates whatever windows live on it. |
| **The display watchdog** (`display_test()`/`display_finish()`, the `_display-watchdog` CLI command) | A layout test applies immediately but isn't written to `displays.toml` until confirmed; an unconfirmed test self-reverts after 15 seconds via a detached `_display-watchdog` process, so a layout that leaves no working input device doesn't strand the user in it. The pending record's `expires` field is vestigial: nothing reads it and no sweeper enforces it. The watchdog process is the only closer of an unconfirmed transaction; if it never runs, the tested layout and pending file survive until the next `display-test` overwrites the record. |
| **The snapshot pattern** (`make_snapshot()`, and the `*_snapshot()` functions it calls) | Each live-read helper is assembled fresh on every call and isolates recoverable failures behind fallback shapes — a settings load failure uses `Defaults::compiled()` with an `error` string rather than taking down every other panel's data alongside it. Read-only but for one: `workspaces_snapshot()` reaches `per_display_groups()` → `workspace_blocks()` → `save_workspace_blocks()`, so a plain `garage snapshot` writes `workspace-blocks.toml` when it meets a connector for the first time. There is one allocator, and reading it means running it. |
| **`coerce_from()`'s leniency policy** | Every bad value is coerced to its shipped default and reported, never rejected. A single error here used to take the whole product down — every render failed, and the one screen that could have fixed the bad value (`set` loads before it writes) went read-only too. |
| **`Paths::mimeapps_override`** | Deliberately not `~/.config/mimeapps.list`: that path is a stow symlink, and every writer of it (including `xdg-mime`) renames a temp file over it, which replaces the symlink with a plain file and cuts it loose from the repo. The desktop-prefixed override file that `set_default_app()` writes wins by XDG spec precedence while the tracked file stays exactly as checked in. |

## 10. Bar extensions

The backend owns bar composition but deliberately does not know the extension catalog. The three
`bar.widgets_*` preference strings are newline-separated opaque ids. `render_bar_layout()` trims
them into the watched `bar-layout.json` marker without validating ids, and `write_marker()` keeps
the inode stable so the running shell sees every settings change. This makes a temporarily missing
or newer extension a presentation concern rather than a settings-load failure.

The shell discovers extensions from two roots. Shipped packages live below the Garage Quickshell
configuration in `extensions/<id>/`; user packages live in
`~/.local/share/garage/extensions/<id>/` and win on an id collision. Each directory has a
`manifest.json` declaring its id, version, capabilities in `provides`, and a `bar-widget` contract.
Unknown composition ids are skipped. A bare icon name resolves to the shipped Phosphor set; a path
is confined to the extension directory. Inline QML is opt-in because the host-owned icon delegate
is the stable default.

Extension QML receives a narrow facade rather than reaching into shell singletons: bar geometry,
screen and theme state, typed spacing, surface/popup openers, the validated manifest, and service
objects. Third-party live data enters through an optional manifest probe. The registry owns one
NDJSON process per probe id for the shell lifetime, independent of monitor count. First-party
collectors use the same service facade and expose availability and error state, so a missing binary
degrades visibly without becoming a respawn loop.

The host, not an extension, owns edge docking, the left/center/right rails, overflow folding, drag
drop-zones, popup placement, and the surface table. That boundary keeps extension packages portable
across horizontal and vertical bars while preserving compatibility IPC shims for existing callers.
