"""The corpus, as data: which invocations the port is held to, grouped by layer.

Scenarios are data and not test methods because the same case has to be run
three times (Python twice for the determinism check, Rust once) against three
freshly built worlds, and because the *set* of them is the thing Phase 3 grows.
A layer task adds cases to its family and, when the layer lands, flips one
boolean. Nothing about the runner changes.

Families are the unit of claimed parity, and `active` is the claim. An inactive
family still runs every one of its cases -- that is the point, it is how progress
becomes visible before it becomes green -- but its mismatches produce a skip
carrying the mismatch count rather than a failure. An active family fails on any
difference that is not written down in deviations.toml. So the file reads, top to
bottom, as a map of how much of the backend has actually been ported: today,
nothing is active, and the smoke family exists to prove the harness itself
compares things.

Each family names the existing Python test files its cases will be grown from.
Those suites already encode what the behaviour is; the differential corpus is not
a place to rediscover it, only to pin it across two implementations.
"""

from __future__ import annotations

import json

from .runner import Family, Scenario


def desktop_entry(name: str, executable: str, mimetypes: str = "",
                  categories: str = "") -> str:
    """A minimal but well-formed .desktop file for the fake machine.

    Planted rather than fixtured because the backend does not *ask* a command
    for these -- desktop_fields() opens the file itself, under XDG_DATA_HOME,
    which the runner clamps into the scratch. So the only way to give a port the
    same applications to find is to put the files there.
    """
    lines = ["[Desktop Entry]", "Type=Application", f"Name={name}",
             f"Exec={executable} %U"]
    if mimetypes:
        lines.append(f"MimeType={mimetypes}")
    if categories:
        lines.append(f"Categories={categories}")
    return "\n".join(lines) + "\n"


# The applications the snapshot scenario's gio fixtures name. Every desktop id
# that appears in fixtures/snapshot-empty.json has a file here: a candidate gio
# reports but whose file is missing is dropped for having no Name, which would
# make the fixture and the corpus quietly disagree.
APPLICATIONS = {
    ".local/share/applications/firefox.desktop": desktop_entry(
        "Firefox", "firefox",
        "text/html;x-scheme-handler/http;x-scheme-handler/https;"),
    ".local/share/applications/thunderbird.desktop": desktop_entry(
        "Thunderbird", "thunderbird", "x-scheme-handler/mailto;"),
    ".local/share/applications/thunar.desktop": desktop_entry(
        "Thunar File Manager", "thunar", "inode/directory;"),
    ".local/share/applications/org.gnome.TextEditor.desktop": desktop_entry(
        "Text Editor", "gnome-text-editor", "text/plain;text/html;"),
    ".local/share/applications/org.gnome.Loupe.desktop": desktop_entry(
        "Image Viewer", "loupe", "image/png;image/jpeg;"),
    ".local/share/applications/mpv.desktop": desktop_entry(
        "mpv Media Player", "mpv", "video/mp4;video/x-matroska;"),
    ".local/share/applications/org.gnome.Evince.desktop": desktop_entry(
        "Document Viewer", "evince", "application/pdf;"),
    # Not a mime handler: the terminal role has no mimetype, so it is resolved
    # from the freedesktop category alone. Present so resolve_terminal() has
    # something to answer with instead of the empty-machine "".
    ".local/share/applications/kitty.desktop": desktop_entry(
        "kitty", "kitty", "", "System;TerminalEmulator;"),
}


# ---------------------------------------------------------------------------
# cli: main()'s own surface, which task 3.15 ported ahead of the layers
# ---------------------------------------------------------------------------
# The first active family, and deliberately the smallest one that can be active:
# every case here is answered by main() itself -- the USAGE text, the argument
# count, the unknown-command message and the envelope they travel in -- with no
# layer underneath it that is still a stub. That is the whole test for whether a
# case belongs here. `snapshot` is the obvious fourth (it is what a bare `garage`
# dispatches to) and it is not here, because make_snapshot() is task 3.9 and the
# case would be claiming parity for something that does not exist yet.
#
# These three moved out of `smoke`, which keeps the cases whose layers are still
# owed. `help` and `unknown-command` were written there to prove the harness
# compares things; they now prove something about the port instead.

CLI = (
    Scenario(
        name="help",
        argv=("help",),
        # USAGE to stdout, exit 0, no filesystem writes and no subprocesses.
        # The narrowest possible case: if this does not compare cleanly, nothing
        # else the harness reports can be trusted.
    ),
    Scenario(
        name="unknown-command",
        argv=("definitely-not-a-command",),
        # The JSON error envelope, exit 1. Pins the *shape* of failure -- ok
        # false with a message -- which is the contract every other command
        # falls back to. Also pins migrate_config_root(), which runs ahead of
        # the dispatch and therefore ahead of this refusal: the digest surface
        # is what says the two backends touch the same nothing on the way past.
    ),
    Scenario(
        name="cli-set-wrong-argc",
        argv=("set", "appearance.accent_color"),
        # main()'s `len(argv) != 4` guard, which is the one `set` refusal that
        # happens before PREFERENCES_LOCK is taken and before the schema is
        # consulted -- so it is answerable today while the rest of `set` is not.
        # The same argv appears in the preferences family as
        # prefs-set-wrong-argument-count, where it is one refusal among many;
        # here it is the claim that the guard itself is ported.
    ),
)


# ---------------------------------------------------------------------------
# smoke: the harness proving itself
# ---------------------------------------------------------------------------
# Not a layer. Three invocations chosen because between them they touch the
# comparison surfaces main()'s own cases cannot: a file-writing command with no
# external calls, a command that is almost entirely external calls, and a command
# that reads, validates, writes and signals.

SMOKE = (
    Scenario(
        name="render-idle-empty",
        argv=("render-idle",),
        # hypridle's ExecStartPre against a HOME with no preferences.toml, so
        # the defaults symlink is the only input. Writes exactly one generated
        # file; spawns nothing. This is the digest surface's simplest exercise.
    ),
    Scenario(
        name="snapshot-empty",
        argv=("snapshot",),
        # The heaviest shim traffic in the corpus: monitors, devices, pipewire,
        # pulse, timedatectl, locale, mime lookups. Almost all of the work is in
        # the trace, which is exactly the surface a port is most likely to get
        # quietly wrong.
        pre_state=dict(APPLICATIONS),
    ),
    Scenario(
        name="set-theme-mode",
        argv=("set", "appearance.theme_mode", '"dark"'),
        # Four argv items, matching main()'s `len(argv) != 4` guard. The full
        # read-validate-write-apply path: takes PREFERENCES_LOCK, rewrites
        # preferences.toml, rewrites the palette markers and signals the
        # session. Both the inode surface (markers truncated in place) and the
        # trace surface (gsettings, hyprctl) carry real information here.
        pre_state={
            # A marker that already exists, so the inode report has something to
            # say about write_marker's in-place truncation.
            ".local/state/garage/generated/accent": "#000000\n",
        },
    ),
)


# ---------------------------------------------------------------------------
# layer families: empty until their Phase 3 task fills them
# ---------------------------------------------------------------------------

# Grown from tests/test_preferences.py and tests/test_schema.py: load, merge over
# defaults, validate, migrate, save. The `set`/`action` argv surface and every
# rejection message.
#
# Two halves, and they become reachable at different times. The load half runs
# through `render-idle` -- the cheapest command that loads the preferences, spawns
# nothing and writes exactly one file -- so every case in it is a statement about
# migrate/merge/validate with no other machinery in the way. What it compares is
# mostly *stderr* (the coercion and dropped-key notes) and *digest*
# (preferences.toml as the migration left it, which for several of these is
# "exactly as it was found"). The `set` half needs main()'s `set` branch, which
# does not exist in the Rust CLI until task 3.15; the cases are written down now so
# that task has a corpus rather than a blank file.
#
# Deliberately absent: two concurrent `set`s. PREFERENCES_LOCK's whole job is to
# serialise them, and a Scenario is one process against one world -- there is
# nowhere to put the second writer. That invariant is held by garage-prefs' own
# unit tests, which contend for the lock in-process (`flock` is per open file
# description, so two opens in one process contend exactly as two processes do).

# A v4 file the way every version up to 4 wrote one: the stamp, a copy of shipped
# defaults, and one real departure. Not the whole ~50-key document -- the property
# under test is that a stored value equal to today's default is dropped while a
# departure stays, and four of each says that as well as fifty does.
V4_FOSSIL = """\
[schema]
preferences_version = 4

[appearance]
accent_color = "red"
corner_radius = "normal"
border_size = 0
theme_mode = "dark"

[bar]
height = 43
"""


def prefs(text: str) -> dict[str, object]:
    """A scenario's starting layer 2."""
    return {".config/garage/preferences.toml": text}


PREFERENCES: tuple[Scenario, ...] = (
    # -- the load path, through the cheapest command that takes it ------------
    Scenario(
        name="prefs-load-departures",
        argv=("render-idle",),
        # Already departures-only and already stamped: nothing to migrate,
        # nothing to rewrite. The control case for every rewrite below.
        pre_state=prefs('[schema]\npreferences_version = 5\n\n'
                        '[appearance]\naccent_color = "red"\n'),
    ),
    Scenario(
        name="prefs-migrate-v4-shrink",
        argv=("render-idle",),
        # v5's whole reason: the fossil shrinks to the one setting ever changed.
        pre_state=prefs(V4_FOSSIL),
    ),
    Scenario(
        name="prefs-migrate-v1-corner-rename",
        argv=("render-idle",),
        # v2's rename and v5's shrink composed: "small" becomes "normal", which
        # is what this build ships, so it leaves the file entirely.
        pre_state=prefs('[appearance]\ncorner_radius = "small"\n'
                        'accent_color = "red"\n'),
    ),
    Scenario(
        name="prefs-migrate-v1-corner-normal",
        argv=("render-idle",),
        # The case a port is most likely to get quietly wrong. "normal" migrates
        # to "large", which is a departure, and the departures the file already
        # holds are the same set it would be rewritten to -- so the comparison
        # says "no change", the file is left unstamped, and the rename replays
        # (idempotently) on every load. The digest surface is what says so.
        pre_state=prefs('[appearance]\ncorner_radius = "normal"\n'),
    ),
    Scenario(
        name="prefs-migrate-v3-wallpaper-split",
        argv=("render-idle",),
        # v3 gave each appearance its own wallpaper. The half that is already
        # set outranks the value it would be split from.
        pre_state=prefs('[schema]\npreferences_version = 2\n\n[appearance]\n'
                        'wallpaper = "/pic.png"\nwallpaper_dark = "/kept.png"\n'),
    ),
    Scenario(
        name="prefs-migrate-unknown-key",
        argv=("render-idle",),
        # Dropped at migration time, with a note on stderr. A stamped file keeps
        # it instead -- see prefs-load-unknown-key-stamped.
        pre_state=prefs('[schema]\npreferences_version = 4\n\n[appearance]\n'
                        'accent_color = "red"\nnot_a_key = 1\n'),
    ),
    Scenario(
        name="prefs-load-unknown-key-stamped",
        argv=("render-idle",),
        # The stamp is what stops the migration, so the unknown key sits in the
        # file, unreported, until something writes it out again.
        pre_state=prefs('[schema]\npreferences_version = 5\n\n[appearance]\n'
                        'accent_color = "red"\nnot_a_key = 1\n'),
    ),
    Scenario(
        name="prefs-migrate-withdrawn-key",
        argv=("render-idle",),
        # highlight_color was withdrawn outright; it leaves with the same note
        # an invented key gets, which is the point of the note's wording.
        pre_state=prefs('[schema]\npreferences_version = 4\n\n'
                        '[appearance]\nhighlight_color = "#ff0000"\n'),
    ),
    Scenario(
        name="prefs-migrate-unknown-section",
        argv=("render-idle",),
        # A whole section the schema does not have is named by the section alone,
        # not key by key.
        pre_state=prefs('[schema]\npreferences_version = 4\n\n'
                        '[appearance]\naccent_color = "red"\n\n[bogus]\nfoo = "bar"\n'),
    ),
    Scenario(
        name="prefs-load-invalid-values",
        argv=("render-idle",),
        # Every one of these is a *departure*, so the compaction leaves them in
        # the file: they are corrected in memory, reported once on stderr, and
        # only reach the file on the next save. Both surfaces carry that.
        pre_state=prefs('[schema]\npreferences_version = 4\n\n[appearance]\n'
                        'corner_radius = "huge"\nborder_size = 99\n'
                        'glass_refraction = 2.5\nnight_shift_start = "25:00"\n'
                        'wallpaper_light_color = "blue"\n\n'
                        '[input]\nkeyboard_layout = ""\n'),
    ),
    Scenario(
        name="prefs-load-exotic-scalars",
        argv=("render-idle",),
        # A hand edit can put a TOML time or a `nan` where a string or a float
        # belongs. Both are reported with the repr Python's f-string produces --
        # `datetime.time(7, 0)`, `nan` -- and neither reaches the emitter,
        # because both are departures and the file is left alone.
        pre_state=prefs('[schema]\npreferences_version = 4\n\n[appearance]\n'
                        'theme_light_at = 07:00:00\nanimation_speed = nan\n'),
    ),
    Scenario(
        name="prefs-load-int-for-float",
        argv=("render-idle",),
        # 1 == 1.0 in Python, and the schema ships pointer_sensitivity as a
        # float: a UI that sends JSON 0 must not pin a copy of the default into
        # layer 2 over a decimal point. The key leaves the file.
        pre_state=prefs('[schema]\npreferences_version = 4\n\n'
                        '[input]\npointer_sensitivity = 0\n'),
    ),
    Scenario(
        name="prefs-load-bool-for-int",
        argv=("render-idle",),
        # The other direction: True == 1 too, but a bool is a different *kind* of
        # value, so it stays a departure and the coercion pass reports it.
        pre_state=prefs('[schema]\npreferences_version = 4\n\n'
                        '[appearance]\nborder_size = true\n'),
    ),
    Scenario(
        name="prefs-load-mangled-stamp",
        argv=("render-idle",),
        # Older than current rather than fatal: refusing the file here would
        # leave the whole UI read-only.
        pre_state=prefs('[schema]\npreferences_version = "5"\n\n'
                        '[appearance]\naccent_color = "red"\n'),
    ),
    Scenario(
        name="prefs-load-hand-written-comments",
        argv=("render-idle",),
        # Already departures-only, so the rewrite would change nothing and the
        # file keeps its comments -- dump_toml() cannot carry one.
        pre_state=prefs('# hand written, keep me\n[schema]\npreferences_version = 5\n\n'
                        '[appearance]\n# why red\naccent_color = "red"\n'),
    ),
    Scenario(
        name="prefs-load-corrupt",
        argv=("render-idle",),
        # The error envelope, and the user's file left exactly as it is.
        pre_state=prefs("this is not toml\n"),
    ),
    # -- v4's file move, which runs once per process ahead of the dispatch -----
    Scenario(
        name="prefs-migrate-config-root",
        argv=("render-idle",),
        # The four user-owned files move out of ~/.config/workstation and the
        # emptied directory goes with them. The symlink is layer 1 and stays,
        # which is also what keeps the directory alive.
        pre_state={
            ".config/workstation/preferences.toml":
                '[appearance]\naccent_color = "red"\n',
            ".config/workstation/displays.toml": "# displays\n",
            ".config/workstation/keybindings.toml": "# keys\n",
            ".config/workstation/workspace-blocks.toml": "# blocks\n",
            ".config/workstation/generated/accent": "#ffffff\n",
        },
    ),
    Scenario(
        name="prefs-migrate-config-root-stamped",
        argv=("render-idle",),
        # Gated on the same stamp as every other step, and read from the *old*
        # file because the new path is still empty at this point. Nothing moves.
        pre_state={
            ".config/workstation/preferences.toml":
                "[schema]\npreferences_version = 4\n",
            ".config/workstation/displays.toml": "# displays\n",
        },
    ),
    Scenario(
        name="prefs-migrate-config-root-occupied",
        argv=("render-idle",),
        # Never over a file already at the new location: that one is what the
        # session has been reading, so the old one is the stale copy.
        pre_state={
            ".config/workstation/preferences.toml": "# old\n",
            ".config/garage/preferences.toml":
                '[schema]\npreferences_version = 5\n\n'
                '[appearance]\naccent_color = "red"\n',
        },
    ),
    # -- the `set` argv surface: one refusal per kind -------------------------
    # Unreachable until task 3.15 wires main()'s `set` branch. Each names the
    # PREFERENCE_KINDS entry it exercises, because the kind is what decides the
    # refusal and one case per kind is what makes the set complete.
    Scenario(name="prefs-set-invalid-bool", argv=("set", "appearance.reduce_motion", '"yes"')),
    Scenario(name="prefs-set-invalid-int", argv=("set", "appearance.border_size", "99")),
    Scenario(name="prefs-set-invalid-float", argv=("set", "appearance.glass_refraction", "2.5")),
    Scenario(name="prefs-set-invalid-enum", argv=("set", "appearance.corner_radius", '"huge"')),
    Scenario(name="prefs-set-invalid-time", argv=("set", "appearance.night_shift_start", '"25:00"')),
    Scenario(name="prefs-set-invalid-hex-color",
             argv=("set", "appearance.wallpaper_light_color", '"blue"')),
    Scenario(name="prefs-set-invalid-nonempty-string",
             argv=("set", "input.keyboard_layout", '""')),
    Scenario(name="prefs-set-invalid-free-string", argv=("set", "general.terminal", "123")),
    Scenario(
        name="prefs-set-invalid-locale",
        argv=("set", "region.locale", '"xx_YY.UTF-8"'),
        # The `locale` shim answers nothing, so no locale is installed and any
        # non-empty one is refused -- deterministically, on any machine.
    ),
    Scenario(
        name="prefs-set-unchecked-value",
        argv=("set", "appearance.night_shift_enabled", '"maybe"'),
        # The "unchecked" kind refuses nothing: the renderer coerces whatever it
        # is handed. Here to pin that it is *accepted*, which is the half of the
        # kind table a port is most likely to get wrong in the safe-looking
        # direction.
    ),
    Scenario(
        name="prefs-set-normalised-counts",
        argv=("set", "workspaces.counts", '"DP-1:99,,HDMI-A-1:x"'),
        # The "normalised" kind restates a value rather than refusing it, so what
        # lands in the file is neither the input nor the default.
    ),
    Scenario(
        name="prefs-set-renamed-value",
        argv=("set", "appearance.corner_radius", '"small"'),
        # A withdrawn spelling is carried across silently -- the same choice under
        # the name it had when it was made -- so this writes "normal" and says
        # nothing on stderr.
    ),
    Scenario(name="prefs-set-renamed-wallpaper-fit",
             argv=("set", "appearance.wallpaper_fit", '"stretch"')),
    Scenario(
        name="prefs-set-unknown-key",
        argv=("set", "nonesuch.key", '"x"'),
        # `set` gates on the schema, so a key the table does not have is refused
        # rather than written.
    ),
    Scenario(
        name="prefs-set-bookkeeping-key",
        argv=("set", "schema.preferences_version", "4"),
        # The version stamp is this program's bookkeeping and is not settable;
        # letting it through would let a `set` replay every migration.
    ),
    Scenario(
        name="prefs-set-wrong-argument-count",
        argv=("set", "appearance.accent_color"),
        # main()'s `len(argv) != 4` guard, which is a usage message rather than a
        # schema refusal.
    ),
    Scenario(
        name="prefs-set-over-a-fossil",
        argv=("set", "appearance.accent_color", '"green"'),
        # The load compacts the file, then the set rewrites it: two writes to
        # preferences.toml in one process, and the second must not reintroduce
        # what the first dropped.
        pre_state=prefs(V4_FOSSIL),
    ),
)

RENDER: tuple[Scenario, ...] = (
    Scenario(
        name="render-all-empty",
        argv=("render",),
        # Task 3.7's own claim: a scratch HOME with no displays.toml runs the whole
        # render_all() chain to completion -- every fragment through render_theme()
        # and render_wallpaper(), then the empty-layout branch that removes (rather
        # than writes) the per-monitor fragment. The one subprocess call anywhere
        # in this path is workspace_outputs()'s `hyprctl monitors -j`, which the
        # fixture answers with no monitors at all -- consistent with "empty".
    ),
)
# Grown from tests/test_bar.py, tests/test_keybinds.py and
# tests/test_schema_table.py: render, render-bar, render-wallpaper and the Lua
# fragments -- the family the luac shim exists for. `render-all-empty` is the
# first case; `render-bar`/`render-wallpaper` proper live in their own families
# below because each is its own CLI command with its own narrow contract.


def bar_prefs(body: str) -> dict[str, object]:
    """A v5-stamped preferences.toml carrying only a `[bar]` departure."""
    return prefs(f"[schema]\npreferences_version = 5\n\n[bar]\n{body}")


RENDER_BAR: tuple[Scenario, ...] = (
    Scenario(
        name="render-bar-defaults",
        argv=("render-bar",),
        # No preferences.toml: the shipped defaults' three widgets (cpu, memory,
        # network), ai_usage and media_player, the workspace indicator, height 43.
    ),
    Scenario(
        name="render-bar-every-widget-off",
        argv=("render-bar",),
        # The minimal bar: every metric strip, the AI strip and the media control
        # all switched off -- widgets.jsonc's definitions shrink to the two empty
        # groups, and workspaces.jsonc carries no "custom/media" entry either.
        pre_state=bar_prefs(
            "monitor_cpu = false\nmonitor_memory = false\nmonitor_network = false\n"
            "ai_usage = false\nmedia_player = false\n"
        ),
    ),
    Scenario(
        name="render-bar-every-metric-on",
        argv=("render-bar",),
        # Every metric strip on at once, in BAR_METRICS' order rather than the
        # schema's declaration order -- the one thing a naive port is likely to
        # get backwards.
        pre_state=bar_prefs(
            "monitor_cpu = true\nmonitor_memory = true\nmonitor_network = true\n"
            "monitor_temp = true\nmonitor_disk = true\nmonitor_gpu = true\n"
        ),
    ),
    Scenario(
        name="render-bar-tall",
        argv=("render-bar",),
        # height at its maximum (60): widgets.jsonc's own "height" field, which
        # config.jsonc no longer names.
        pre_state=bar_prefs("height = 60\n"),
    ),
    Scenario(
        name="render-bar-short",
        argv=("render-bar",),
        # height at its minimum (30).
        pre_state=bar_prefs("height = 30\n"),
    ),
    Scenario(
        name="render-bar-padding-scale-loose",
        argv=("render-bar",),
        # padding_scale is a style.css concern, not a widgets/workspaces one --
        # render-bar does not write style.css at all, so this is the control case
        # proving the scale is invisible to the three fragments this command owns.
        pre_state=bar_prefs("padding_scale = 2.0\n"),
    ),
    Scenario(
        name="render-bar-background-transparent",
        argv=("render-bar",),
        # Same control, for bar.background: also a style.css-only key.
        pre_state=bar_prefs('background = "transparent"\n'),
    ),
    Scenario(
        name="render-bar-no-workspace-indicator",
        argv=("render-bar",),
        # workspaces.indicator lives in [workspaces], not [bar], and is the one
        # switch render_bar_workspaces() itself reads: modules-left drops
        # "ext/workspaces" and keeps the menu.
        pre_state=prefs(
            "[schema]\npreferences_version = 5\n\n[workspaces]\nindicator = false\n"
        ),
    ),
)


def wallpaper_prefs(body: str) -> dict[str, object]:
    """A v5-stamped preferences.toml carrying an `[appearance]` departure."""
    return prefs(f"[schema]\npreferences_version = 5\n\n[appearance]\n{body}")


RENDER_WALLPAPER: tuple[Scenario, ...] = (
    Scenario(
        name="render-wallpaper-light-image",
        argv=("render-wallpaper",),
        # theme_mode pins the resolved scheme to light without needing the clock.
        # An image source respects wallpaper_fit -- "contain" here, not the
        # shipped "cover" -- which is the one thing a color source overrides.
        pre_state=wallpaper_prefs(
            'theme_mode = "light"\nwallpaper_fit = "contain"\n'
            'wallpaper_light_source = "image"\nwallpaper_light = "/pic.png"\n'
        ),
    ),
    Scenario(
        name="render-wallpaper-dark-image",
        argv=("render-wallpaper",),
        # The dark half of the same, with a different fit ("tile") so a
        # transposition between the two appearances would be visible.
        pre_state=wallpaper_prefs(
            'theme_mode = "dark"\nwallpaper_fit = "tile"\n'
            'wallpaper_dark_source = "image"\nwallpaper_dark = "/pic-dark.png"\n'
        ),
    ),
    Scenario(
        name="render-wallpaper-light-color",
        argv=("render-wallpaper",),
        # wallpaper_fit is still "contain", but a colour source has no composition
        # to preserve, so render_wallpaper() forces "cover" regardless -- the
        # override this family exists to pin.
        pre_state=wallpaper_prefs(
            'theme_mode = "light"\nwallpaper_fit = "contain"\n'
            'wallpaper_light_source = "color"\nwallpaper_light_color = "#ff0000"\n'
        ),
    ),
    Scenario(
        name="render-wallpaper-dark-color",
        argv=("render-wallpaper",),
        pre_state=wallpaper_prefs(
            'theme_mode = "dark"\nwallpaper_fit = "fit"\n'
            'wallpaper_dark_source = "color"\nwallpaper_dark_color = "#00ff00"\n'
        ),
    ),
)


def stamped_lock_prefs(body: str) -> dict[str, object]:
    """A v5-stamped preferences.toml carrying only a `[lock]` departure, so the load
    path takes no migration branch and the scenario is purely about what render_idle
    does with the timeouts."""
    return prefs(f"[schema]\npreferences_version = 5\n\n{body}")


RENDER_IDLE: tuple[Scenario, ...] = (
    Scenario(
        name="render-idle-defaults",
        argv=("render-idle",),
        # No preferences.toml at all: the shipped defaults' lock_timeout=600 and
        # display_off_timeout=900 each produce a listener; suspend_timeout=0 does not.
    ),
    Scenario(
        name="render-idle-lock-all-zero",
        argv=("render-idle",),
        # Every timeout at zero: hypridle.conf carries the general{} block and no
        # listener at all -- the case task 3.3's own unit tests could not cover without
        # a toml dev-dependency this crate does not otherwise need.
        pre_state=stamped_lock_prefs(
            "[lock]\nlock_timeout = 0\ndisplay_off_timeout = 0\nsuspend_timeout = 0\n"
        ),
    ),
    Scenario(
        name="render-idle-lock-all-nonzero",
        argv=("render-idle",),
        # All three configured: three listener blocks, in lock/display-off/suspend
        # order, each with a distinct timeout so a transposition would be visible.
        pre_state=stamped_lock_prefs(
            "[lock]\nlock_timeout = 300\ndisplay_off_timeout = 120\n"
            "suspend_timeout = 1800\n"
        ),
    ),
    Scenario(
        name="render-idle-suspend-only",
        argv=("render-idle",),
        # Only suspend configured -- the one listener the shipped defaults never
        # exercise, since suspend_timeout defaults to 0.
        pre_state=stamped_lock_prefs(
            "[lock]\nlock_timeout = 0\ndisplay_off_timeout = 0\nsuspend_timeout = 600\n"
        ),
    ),
)

PALETTE: tuple[Scenario, ...] = ()
# Grown from tests/test_palette.py and tests/test_wallpapers.py: theme-sync,
# night-shift-sync, wallpaper selection, and the marker files whose *inodes*
# Quickshell watches.

APPLY: tuple[Scenario, ...] = ()
# Grown from tests/test_session_surfaces.py: apply, and every gsettings /
# hyprctl / systemctl call it makes. Almost purely a trace-surface family.

# The layout displays.toml starts on for the cases that plant one: the same two monitors the
# fixture reports, side by side, so a revert lands back on exactly what was already there.
SAVED_DISPLAYS = """\
primary = "DP-1"

[[display]]
output = "DP-1"
description = "Acme Displays 27 (DP-1)"
enabled = true
mode = "1920x1080@59.996"
x = 0
y = 0
scale = 1.0
transform = 0
vrr = -1

[[display]]
output = "HDMI-A-1"
description = "Acme Displays 24 (HDMI-A-1)"
enabled = true
mode = "1920x1080@60"
x = 1920
y = 0
scale = 1.0
transform = 0
vrr = -1
"""

# The candidate every display-test below sends: the same two displays stacked rather than side
# by side, which is a different arrangement, still edge-to-edge, and therefore accepted.
STACKED_PAYLOAD = json.dumps({
    "primary": "HDMI-A-1",
    "displays": [
        {"output": "DP-1", "description": "Acme Displays 27 (DP-1)", "enabled": True,
         "mode": "1920x1080@59.996", "x": 0, "y": 0, "scale": 1.0, "transform": 0,
         "vrr": -1, "width": 1920, "height": 1080, "mirror": ""},
        {"output": "HDMI-A-1", "description": "Acme Displays 24 (HDMI-A-1)", "enabled": True,
         "mode": "1920x1080@60", "x": 0, "y": 1080, "scale": 1.0, "transform": 0,
         "vrr": -1, "width": 1920, "height": 1080, "mirror": ""},
    ],
})

# A candidate refused on the way *in*, before display_test() has written anything: `float(x)`
# raises on it, which happens inside normalize_display_layout() and therefore ahead of the
# pending record, the apply and the watchdog.
#
# Deliberately not the overlapping-layout refusal, which would be the more obvious case: that
# one is refused by apply_display_layout(), which runs *after* the pending record is written,
# so it leaves a transaction open -- and an open transaction holds a fresh uuid and a
# `time.time()` expiry, which two runs of the Python backend cannot agree on. It is a
# determinism failure rather than a comparison, and it is what the harness's own
# Python-vs-Python check is for. Every geometry refusal is covered exhaustively instead by
# garage-apply's display_traces.json corpus, which drives the same code in-process with a fake
# clock and a fake runner.
REFUSED_PAYLOAD = json.dumps({
    "primary": "DP-1",
    "displays": [
        {"output": "DP-1", "enabled": True, "x": "left", "y": 0, "scale": 1.0,
         "width": 1920, "height": 1080},
    ],
})


def displays(text: str) -> dict[str, object]:
    """A scenario's starting displays.toml."""
    return {".config/garage/displays.toml": text}


DISPLAYS: tuple[Scenario, ...] = (
    Scenario(
        name="display-test-confirm",
        argv=("display-test", STACKED_PAYLOAD),
        then=("display-confirm", "$TOKEN"),
        fixtures="displays-two-monitors",
        pre_state=displays(SAVED_DISPLAYS),
        # The whole transaction, committed. Two processes against one world: the first writes
        # the pending record, applies the candidate and spawns the watchdog; the second writes
        # displays.toml, renders the fragment twice (once itself, once inside
        # apply_display_layout) and takes the pending record away. The digest is what says the
        # saved layout is the candidate's, and the trace is what says the compositor was
        # reloaded twice and asked for its monitors exactly once.
    ),
    Scenario(
        name="display-test-revert",
        argv=("display-test", STACKED_PAYLOAD),
        then=("display-revert", "$TOKEN"),
        fixtures="displays-two-monitors",
        pre_state=displays(SAVED_DISPLAYS),
        # The same transaction, rolled back -- the watchdog's own path, since the watchdog is
        # display_finish(token, False) and nothing else. displays.toml must come back
        # untouched (the inode surface says so as well as the digest), and the fragment must be
        # the one rendered from the *previous* arrangement, which is display_snapshot()'s
        # records rather than the file's -- so this is also the byte-parity test for the
        # snapshot's mode string, its %g refresh rate and its saved-vrr fold.
    ),
    Scenario(
        name="display-test-refused",
        argv=("display-test", REFUSED_PAYLOAD),
        fixtures="displays-two-monitors",
        pre_state=displays(SAVED_DISPLAYS),
        # A candidate refused before display_test() writes anything at all: the error envelope
        # and exit 1, the compositor never asked, nothing on disk, no watchdog. See
        # REFUSED_PAYLOAD for why this is the coercion refusal rather than a geometry one.
    ),
    Scenario(
        name="display-confirm-unknown-token",
        argv=("display-confirm", "0123456789abcdef0123456789abcdef"),
        fixtures="displays-two-monitors",
        pre_state=displays(SAVED_DISPLAYS),
        # No transaction is open at all, which is the watchdog-fires-after-a-confirm case
        # stated as one process: display_finish() returns without doing anything, prints the
        # success envelope, and touches nothing but the lock file it took on the way in.
    ),
    Scenario(
        name="display-test-no-saved-layout",
        argv=("display-test", STACKED_PAYLOAD),
        then=("display-confirm", "$TOKEN"),
        fixtures="displays-two-monitors",
        # No displays.toml at all: the previous arrangement can only come from the compositor,
        # and the confirm is the first thing that ever writes the file.
    ),
)
# Grown from tests/test_recovery.py: display-test, display-confirm,
# display-revert and the watchdog, plus displays.toml round-tripping.

FILES: tuple[Scenario, ...] = ()
# Grown from tests/test_files.py, tests/test_file_index.py and
# tests/test_launcher.py: the default-application roles and the mime writes.

# ---------------------------------------------------------------------------
# doctor: the three commands that print lines for a person
# ---------------------------------------------------------------------------
# Grown from tests/test_recovery.py, tests/test_manifest.py and tests/test_docs.py.
# Their stdout is compared verbatim, which is the whole point of the family: these
# are the only commands whose output a person reads.
#
# WHAT IS DELIBERATELY NOT HERE, and it is three things rather than an oversight:
#
#   * `doctor --report`. Its first field is `generated_at`, a wall-clock stamp, so
#     two runs of the *same* backend disagree and the harness's determinism check
#     fails before any parity verdict is reached. The JSON surface is pinned
#     instead by backend/crates/garage-apply/src/doctor/parity.rs, which compares
#     it byte for byte against the Python's own output with the clock blanked.
#   * `repair` against a file that exists. The transcript carries that file's
#     mtime, which is the moment the harness planted it -- the same problem. The
#     fresh-install cases below have no file and therefore no timestamp, and the
#     full transcript corpus (broken file, --reset, the backup-collision naming)
#     lives in garage-apply's repair fixtures.
#   * `update` in any form that gets past argument parsing. It hands the terminal
#     to bootstrap.sh even under --dry-run -- bootstrap has its own --dry-run and
#     its answer is the authoritative one -- and running the real bootstrap.sh
#     against a scratch $HOME is exactly what a differential harness must not do.
#     Its traces are pinned by garage-apply's update fixtures, against a fake
#     runner.

DOCTOR = (
    Scenario(
        name="doctor-healthy",
        argv=("doctor",),
        fixtures="doctor-healthy",
        # The aligned transcript, and the exit status that answers "is this
        # install healthy". The scratch $HOME has none of the checkout's managed
        # paths in it, so `stow links` and `dead links` are the real work here:
        # both backends walk the *repository's* desktop/ tree (checkout_root() is
        # resolved from the running binary, and both binaries sit three levels
        # inside this checkout), classify every managed path as missing, and have
        # to agree on which ten of them are shown and how many are counted.
    ),
    Scenario(
        name="doctor-unknown-argument",
        argv=("doctor", "--repot"),
        # The catch tier: `garage doctor: {error}` on stderr, exit 1, and nothing
        # on stdout. The one refusal shape all three plumbing commands share.
    ),
    Scenario(
        name="repair-fresh-install",
        argv=("repair",),
        # No preferences.toml at all, which is why this case and not a broken one:
        # the "does not exist" branch prints no size and no mtime, so the whole
        # transcript is a function of the build rather than of the clock.
    ),
    Scenario(
        name="repair-reset-fresh-install",
        argv=("repair", "--reset"),
        # "backup none needed; there was no file to keep", then the stamp-only
        # file. The digest surface is what proves the two backends wrote the same
        # factory state -- under the deltas model that is a file with the schema
        # stamp and nothing else.
    ),
    Scenario(
        name="repair-unknown-argument",
        argv=("repair", "--rest"),
        # Refused before the lock is taken and before anything is backed up,
        # which is what test_recovery.py's own version of this asserts.
    ),
    Scenario(
        name="update-unknown-argument",
        argv=("update", "--dryrun"),
        # The only `update` case that can be run here at all: argument parsing
        # happens before checkout_root(), before the sweep and long before
        # bootstrap.sh. See the note above.
    ),
)


FAMILIES = (
    Family(
        name="cli",
        active=True,
        note="main()'s dispatch, USAGE and the JSON envelope; task 3.15",
        scenarios=CLI,
    ),
    Family(
        name="smoke",
        active=False,
        note="the harness proving it compares things; not a layer claim",
        scenarios=SMOKE,
    ),
    Family(name="preferences", active=False,
           note="load/validate/save; from test_preferences.py, test_schema.py. "
                "The load half is ported (task 3.1, garage-prefs) and reaches the "
                "CLI through render-idle (task 3.15); the `set` half is dispatched "
                "and writes the file, but its route walk ends in whichever renderer "
                "or applier is still owed. Activated by task 3.2.",
           scenarios=PREFERENCES),
    Family(name="render", active=True,
           note="`garage render`; task 3.7 wired render_all() to completion for a scratch "
                "tree with no displays.toml. render_displays() itself is still owed, which "
                "is why the one case here is deliberately the empty-layout branch.",
           scenarios=RENDER),
    Family(name="render_idle", active=True,
           note="hypridle.conf from [lock] alone; task 3.3, garage-render. Active because "
                "`garage render-idle` now runs the real load -> render_idle chain end to "
                "end (task 3.15's CLI wiring).",
           scenarios=RENDER_IDLE),
    Family(name="render_bar", active=True,
           note="`garage render-bar`; task 3.7 wired render_region() + "
                "render_bar_workspaces() + render_bar_widgets() to the CLI.",
           scenarios=RENDER_BAR),
    Family(name="render_wallpaper", active=True,
           note="`garage render-wallpaper`; task 3.7 wired render_wallpaper() to the CLI.",
           scenarios=RENDER_WALLPAPER),
    Family(name="palette", active=False,
           note="theme and wallpaper markers; from test_palette.py",
           scenarios=PALETTE),
    Family(name="apply", active=False,
           note="session signalling; from test_session_surfaces.py",
           scenarios=APPLY),
    Family(name="displays", active=True,
           note="display test/confirm/revert and the fifteen-second watchdog; task 3.8 "
                "ported load_display_config(), mirror_targets() and render_displays() into "
                "garage-render and the transaction, the geometry check, the snapshot and the "
                "seeding into garage-apply, and wired all four commands in the CLI.",
           scenarios=DISPLAYS),
    Family(name="files", active=False,
           note="default applications and mime; from test_files.py",
           scenarios=FILES),
    Family(name="doctor", active=True,
           note="`garage doctor`, `garage repair` and `garage update`; tasks 3.12-3.14 "
                "ported all three and wired them to the CLI's plain-command arms. Active "
                "with two written-down deviations, both of them the same deliberate "
                "departure: the Rust doctor reads system/manifest/*.list at runtime where "
                "the Python names its own DOCTOR_* tuples, and the file is the wider of "
                "the two. From test_recovery.py, test_manifest.py, test_docs.py.",
           scenarios=DOCTOR),
)
