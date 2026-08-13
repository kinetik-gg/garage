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
bottom, as a map of how much of the backend has actually been ported: as of tasks
3.10 and 3.11 every family is active, and what the port is forgiven is the whole
of deviations.toml and nothing else.

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
# The first family that was ever active, and deliberately the smallest one that
# could be: every case here is answered by main() itself -- the USAGE text, the
# argument count, the unknown-command message and the envelope they travel in --
# with no layer underneath it at all. That is the whole test for whether a case
# belongs here, and it is why `snapshot` is not one of them even now that
# make_snapshot() is ported: a bare `garage` does dispatch to it, but the case
# would then be a claim about eight live reads rather than about the dispatch.
# It lives in `snapshot` instead, with `snapshot-default-command` beside it
# pinning the bare-argv default.

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
)
# `snapshot-empty` and `set-theme-mode` used to be here, as the two cases whose
# layers were still owed. Task 3.10+3.11 ported both, so each moved to the family
# that claims it: `snapshot-empty` to `snapshot`, `set-theme-mode` to `palette`.
# What is left is the one case that was never a layer claim -- a file-writing
# command with no external calls, which is what proves the digest surface works.


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

def stamped(body: str) -> dict[str, object]:
    """A v5-stamped preferences.toml, so the load takes no migration branch and
    the scenario is purely about what the apply layer does with the values."""
    return prefs(f"[schema]\npreferences_version = 5\n\n{body}")


# Every scenario below pins theme_mode explicitly rather than leaving it on
# `auto`. Auto reads the wall clock, and a case that straddled 07:00 or 19:00
# between the harness's two Python runs would fail the determinism check rather
# than say anything about the port. Night shift is pinned the same way and for
# the same reason: `night_shift_enabled = false` is always inactive, and a
# start equal to its end falls into the wrapping branch, which is always active.

PALETTE: tuple[Scenario, ...] = (
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
    Scenario(
        name="set-accent-color",
        argv=("set", "appearance.accent_color", '"teal"'),
        fixtures="session",
        # Route::Accent, which is the one route whose applier writes its own
        # marker rather than going through a RenderStep -- render_accent() then
        # push_accent(). The `gsettings range` gate is what the fixture is for:
        # the name is in the range, so the push happens.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="set-accent-color-out-of-range",
        argv=("set", "appearance.accent_color", '"slate"'),
        # No fixture at all, so `gsettings range` answers empty and the name is
        # not in it: the marker is still written and the push is skipped. The
        # half of push_accent() a port is most likely to drop.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="theme-sync-first-run",
        argv=("theme-sync",),
        fixtures="session",
        # Nothing has ever been pushed, so applied_scheme() is "" and the gate
        # is open: the whole palette is written, the seven signals are issued,
        # render_preferences() runs again and the compositor is reloaded. The
        # response carries the resolved scheme.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="theme-sync-already-applied",
        argv=("theme-sync",),
        fixtures="session",
        # The gate closed: the marker already says "dark". Nothing is written and
        # nothing is signalled, which is the whole point of the five-minute timer
        # not making the desktop flicker. The response is still the scheme.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n')
        | {".local/state/garage/generated/color-scheme": "dark\n"},
    ),
    Scenario(
        name="theme-sync-scheme-moved",
        argv=("theme-sync",),
        fixtures="session",
        # The marker says dark and the schedule says light: the full switch, with
        # the marker overwritten in place (the inode surface says so, because
        # Quickshell is watching that inode).
        pre_state=stamped('[appearance]\ntheme_mode = "light"\n')
        | {".local/state/garage/generated/color-scheme": "dark\n"},
    ),
    Scenario(
        name="night-shift-sync-active",
        argv=("night-shift-sync",),
        fixtures="session",
        # start == end is not a window at all, and the Python's wrapping branch
        # reads it as every minute of the day -- which is what makes this case
        # answerable without a clock. The temperature reaches hyprsunset.
        pre_state=stamped(
            '[appearance]\nnight_shift_enabled = true\n'
            'night_shift_start = "00:00"\nnight_shift_end = "00:00"\n'
            'night_shift_temperature = 3800\n'
        ),
    ),
    Scenario(
        name="night-shift-sync-disabled",
        argv=("night-shift-sync",),
        fixtures="session",
        # The other arm: identity, and `active` false in the response.
        pre_state=stamped("[appearance]\nnight_shift_enabled = false\n"),
    ),
    Scenario(
        name="set-wallpaper-solid-colour",
        argv=("set", "appearance.wallpaper_dark_color", '"#ff0000"'),
        fixtures="session",
        # Route::WallpaperDark against a dark session, so the live half lands.
        # hyprpaper has no colour mode, so the swatch is rendered to a PNG named
        # after it -- the `magick` argv is pinned byte for byte here, geometry
        # included (3840x2160, the independent maxima of the fixture's two
        # displays, which is neither display's own resolution). The shim writes
        # no file, so the "exited zero and wrote nothing" guard fires and the
        # envelope carries `Unable to render #ff0000`.
        pre_state=stamped(
            '[appearance]\ntheme_mode = "dark"\nwallpaper_dark_source = "color"\n'
        ),
    ),
    Scenario(
        name="set-wallpaper-missing-picture",
        argv=("set", "appearance.wallpaper_dark", '"/nope/absent.png"'),
        fixtures="session",
        # The other refusal wallpaper_target() can raise, in the Python's own
        # words. The file is still written before the route walks, which is the
        # property the digest surface is here for.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="set-wallpaper-fit",
        argv=("set", "appearance.wallpaper_fit", '"contain"'),
        fixtures="session",
        # The shared fit, which always reaches the desktop: hyprpaper.conf moves,
        # so this is the restart arm rather than the IPC one. A picture is planted
        # so wallpaper_target() resolves to something.
        pre_state=stamped(
            '[appearance]\ntheme_mode = "dark"\nwallpaper_dark = "~/pic.png"\n'
        )
        | {"pic.png": "not really a png, but `file` is fixtured\n"},
    ),
    Scenario(
        name="set-wallpaper-picture-unchanged-fit",
        argv=("render-wallpaper",),
        then=("set", "appearance.wallpaper_dark", '"~/pic.png"'),
        fixtures="session",
        # Two commands against one world, which is the only way to reach the IPC
        # arm: `render-wallpaper` writes hyprpaper.conf first, so by the time the
        # `set` runs the file already says what render_wallpaper() would write.
        # The fit therefore did not move, and the change goes over IPC instead --
        # one `hyprctl hyprpaper wallpaper <monitor>,<resolved>` per monitor, with
        # the *resolved* target rather than the `current` symlink, which is what
        # hyprpaper's path-keyed cache needs to notice a change at all. Planting
        # the file directly is not an option: its one line is an absolute path
        # into a scratch HOME the harness invents per run.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n')
        | {"pic.png": "not really a png, but `file` is fixtured\n"},
    ),
)
# Grown from tests/test_palette.py and tests/test_wallpapers.py: theme-sync,
# night-shift-sync, wallpaper selection, and the marker files whose *inodes*
# Quickshell watches. Task 3.10+3.11 ported push_theme(), apply_wallpaper() and
# the whole wallpaper resolver, which is what made every case here answerable.

APPLY: tuple[Scenario, ...] = (
    Scenario(
        name="apply-empty",
        argv=("apply",),
        fixtures="session",
        # The session-start path from autostart.lua, against a machine that has
        # never been configured: seed displays.toml from the live compositor,
        # render hyprpaper.conf *ahead* of render_all() so the restart decision
        # is taken by the first writer, render everything, then push accent,
        # corner radius and theme, reload, dress the wallpaper, put the night
        # shift schedule in, and restart hypridle. Almost all of it is trace.
        pre_state=stamped(
            '[appearance]\ntheme_mode = "dark"\nnight_shift_enabled = false\n'
        ),
    ),
    Scenario(
        name="apply-night-shift-on",
        argv=("apply",),
        fixtures="session",
        # The other arm of the one step in the sequence that reads a schedule.
        # start == end is always active, so this needs no clock -- see PALETTE's
        # own note.
        pre_state=stamped(
            '[appearance]\ntheme_mode = "light"\nnight_shift_enabled = true\n'
            'night_shift_start = "09:00"\nnight_shift_end = "09:00"\n'
            'night_shift_temperature = 3200\n'
        ),
    ),
    Scenario(
        name="apply-with-a-wallpaper",
        argv=("apply",),
        fixtures="session",
        # The wallpaper half reaches the end: the `current` symlink is re-pointed
        # atomically, hyprpaper.conf is new so the service is restarted rather
        # than talked to, and the digest carries the symlink the two backends both
        # wrote. `paper_moved` is threaded from ahead of render_all(), which is
        # the ordering this case exists to pin.
        pre_state=stamped(
            '[appearance]\ntheme_mode = "dark"\nnight_shift_enabled = false\n'
            'wallpaper_dark = "~/pic.png"\n'
        )
        | {"pic.png": "not really a png, but `file` is fixtured\n"},
    ),
    Scenario(
        name="apply-shared-workspaces",
        argv=("apply",),
        fixtures="session",
        # Shared mode, which changes what the workspace half of a full render
        # emits and leaves the block allocator with one group rather than one per
        # display. Here because `apply` is the only command that runs the whole
        # chain, and a mode difference this deep in it should be visible on the
        # digest without a second command.
        pre_state=stamped(
            '[appearance]\ntheme_mode = "dark"\nnight_shift_enabled = false\n\n'
            '[workspaces]\nmode = "shared"\nshared_count = 6\n'
        ),
    ),
    Scenario(
        name="apply-file-index-off",
        argv=("set", "indexing.enabled", "false"),
        fixtures="session",
        # Not `apply`: Route::FileIndex is the one route whose every step is a
        # run_or_raise, so this is where the three `systemctl --user` calls and
        # their named refusals live. Disabling stops and disables in one call and
        # returns; enabling is enable-then-restart.
        pre_state=stamped("[indexing]\nenabled = true\n"),
    ),
    Scenario(
        name="apply-file-index-on",
        argv=("set", "indexing.frequency_minutes", "10"),
        fixtures="session",
        pre_state=stamped("[indexing]\nenabled = true\n"),
    ),
    Scenario(
        name="apply-locale",
        argv=("set", "region.locale", '"id_ID.UTF-8"'),
        fixtures="session",
        # Route::Locale is two apply steps: seed the systemd user manager's
        # environment (and the D-Bus activation environment beside it), then
        # render the region fragments and reload the bar. The `locale -a` fixture
        # is what makes id_ID.UTF-8 an installed locale rather than a refused one.
        pre_state=stamped(""),
    ),
    Scenario(
        name="apply-locale-cleared",
        argv=("set", "region.locale", '""'),
        fixtures="session",
        # The empty override resolves to the *system* locale rather than clearing
        # LANG to nothing, which is the arm a port is most likely to write as
        # `set-environment LANG=`. `/etc/locale.conf` is the one absolute path
        # neither backend clamps into the scratch, so the value here is the
        # developer's own -- which is fine, because what is compared is that the
        # two backends read the same file and agree, not what it happens to say.
        #
        # No stored locale, deliberately: the Python's `locale` kind consults
        # `locale -a` for a *non-empty* value, so a departure would put that read
        # on the load's trace too. `apply-locale` above is where that difference
        # is exercised and written down; this case is about the unset arm alone.
        pre_state=stamped(""),
    ),
    Scenario(
        name="apply-region-date-format",
        argv=("set", "region.date_format", '"mdy"'),
        fixtures="session",
        # Route::Region: the same renderer, one step fewer. Every region key
        # reaches the bar clock, and only the locale takes the extra step above.
        pre_state=stamped(""),
    ),
    Scenario(
        name="apply-glass-mode",
        argv=("set", "appearance.glass_mode", '"frosted"'),
        fixtures="session",
        # The material, pushed live: the marker Quickshell watches, then one
        # `hyprctl eval` carrying both the core decoration options and the plugin
        # ones, with the blur write-back appended. The eval body is the thing this
        # pins -- it has to be the same Lua the fragment carries.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="apply-glass-eval-refused",
        argv=("set", "appearance.glass_mode", '"off"'),
        fixtures="glass-eval-refused",
        # The plugin never loaded, so the eval fails as a whole and the fallback
        # is a full reload rather than refusing the setting. The reload succeeds
        # here, so the envelope is still ok.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="apply-corner-radius",
        argv=("set", "appearance.corner_radius", '"large"'),
        fixtures="session",
        # Render the marker, then push it: one eval across the core decoration
        # options, the Kinetik Glass plugin's and hyprexpo's together, with
        # CORNER_POWER spelled the way `f"{3.37:g}"` spells it.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="apply-border-size",
        argv=("set", "appearance.border_size", "3"),
        fixtures="session",
        # The size and the theme-resolved colour together, because decorations.lua
        # paints both borders fully transparent and a size with no colour would
        # silently shrink the window and draw nothing.
        pre_state=stamped('[appearance]\ntheme_mode = "light"\n'),
    ),
    Scenario(
        name="apply-motion",
        argv=("set", "appearance.reduce_motion", "true"),
        fixtures="session",
        # The one applier that deliberately does *not* go through eval_config():
        # per-leaf speeds are top-level hl.animation() calls rather than a single
        # hl.config table, and none of the blur write-back is wanted. Route::Motion
        # also rewrites the bar's own Reduce Motion and reloads it.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="apply-bar-style",
        argv=("set", "bar.padding_scale", "2.0"),
        fixtures="session",
        # The narrow stylesheet applier: one file, written in place because waybar
        # watches it, and one SIGUSR2. Deliberately not a theme render, which would
        # rewrite twenty toolkit configs per slider notch.
        pre_state=stamped('[appearance]\ntheme_mode = "dark"\n'),
    ),
    Scenario(
        name="apply-bar-widgets",
        argv=("set", "bar.media_player", "false"),
        fixtures="session",
        # Media spans two fragments -- its definition is widget-owned, its place
        # after the workspace indicator is left-side-owned -- so both are
        # republished before the one reload.
        pre_state=stamped(""),
    ),
    Scenario(
        name="apply-bar-workspaces",
        argv=("set", "workspaces.indicator", "false"),
        fixtures="session",
        # The other bar route: the module list alone, and the compositor untouched.
        pre_state=stamped(""),
    ),
    Scenario(
        name="apply-workspace-plan",
        argv=("set", "workspaces.default_count", "6"),
        fixtures="session",
        # The salvage choreography, end to end through the CLI: read the clients,
        # remap, reap, render, reload, restore. garage-apply's own
        # workspace_traces.json drives the same code in-process with a fake runner;
        # this is the claim that the command reaches it.
        pre_state=stamped(""),
    ),
    Scenario(
        name="apply-input",
        argv=("set", "input.pointer_sensitivity", "0.4"),
        fixtures="session",
        # The whole input section reaches the compositor through a plain reload,
        # which is the shortest run_or_raise route in the table.
        pre_state=stamped(""),
    ),
    Scenario(
        name="apply-launcher",
        argv=("set", "general.builtin_launcher", "false"),
        fixtures="session",
        # Route::Launcher signals nothing at all: the wrapper reads the marker on
        # every press and the shell watches it, so the switch takes effect as it is
        # written. The one route whose trace is empty, which is worth pinning
        # precisely because an over-eager port would add a reload.
        pre_state=stamped(""),
    ),
)
# Grown from tests/test_session_surfaces.py: apply, and every gsettings /
# hyprctl / systemctl call it makes. Almost purely a trace-surface family, and
# the family task 3.10+3.11 exists for -- every route in PREFERENCE_ROUTES that
# ends in a signal has a case here or in `palette`.

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

FILES: tuple[Scenario, ...] = (
    Scenario(
        name="set-terminal",
        argv=("set", "general.terminal", '"kitty.desktop"'),
        fixtures="session",
        pre_state=stamped("") | dict(APPLICATIONS),
        # Route::Terminal, and the case that settles where the *browser* marker is
        # written. The Python's render_general() writes three markers and this
        # applier calls it; the port's renderer writes two, because resolving a
        # browser association runs `gio mime` and a renderer structurally has no
        # runner -- so the third is written from the apply side, at the same point
        # in the same order. All three markers and the three `gio mime` calls are
        # on this trace in both backends.
    ),
    Scenario(
        name="action-defaults-files",
        argv=("action", "defaults.files", '"thunar.desktop"'),
        fixtures="session",
        pre_state=dict(APPLICATIONS),
        # A non-browser role: the mimeapps override is rewritten and nothing else
        # happens -- no marker, no reload. The counterpart to
        # `action-defaults-browser` in the `actions` family, which does both.
    ),
    Scenario(
        name="action-defaults-not-installed",
        argv=("action", "defaults.files", '"nautilus.desktop"'),
        fixtures="session",
        pre_state=dict(APPLICATIONS),
        # `{desktop_id} is not installed`, refused before anything is written.
    ),
)
# Grown from tests/test_files.py, tests/test_file_index.py and
# tests/test_launcher.py: the default-application roles and the mime writes.

# ---------------------------------------------------------------------------
# actions: `garage action NAME [JSON]`, the second dispatch table
# ---------------------------------------------------------------------------
# An action is not a preference: it has no key in preferences.toml and no route
# of its own. Most of these either read from or write straight to the world --
# wpctl, pactl, loginctl, timedatectl -- with nothing to persist, so the family is
# almost purely trace. The two that *are* read-modify-writes under the preferences
# lock (`appearance.night_shift.toggle` and `glass.reset`) carry the digest too.

ACTIONS: tuple[Scenario, ...] = (
    Scenario(
        name="action-night-shift-toggle",
        argv=("action", "appearance.night_shift.toggle"),
        fixtures="session",
        # The one action that inverts a stored boolean and then walks that key's
        # ordinary route. The pane fires this rather than a `set` because it does
        # not know the current value and must not race a second toggle into
        # reading the same one twice. Off here, so it comes back on and the
        # temperature reaches hyprsunset -- start == end, so no clock is involved.
        pre_state=stamped(
            '[appearance]\nnight_shift_enabled = false\n'
            'night_shift_start = "12:00"\nnight_shift_end = "12:00"\n'
        ),
    ),
    Scenario(
        name="action-night-shift-toggle-off",
        argv=("action", "appearance.night_shift.toggle"),
        fixtures="session",
        # The other direction: on becomes off, and the identity call goes out.
        # The digest is what says the file now carries the departure.
        pre_state=stamped(
            '[appearance]\nnight_shift_enabled = true\n'
            'night_shift_start = "12:00"\nnight_shift_end = "12:00"\n'
        ),
    ),
    Scenario(
        name="action-glass-reset",
        argv=("action", "glass.reset"),
        fixtures="session",
        # Every `glass_*` key walked back to its shipped default, under the
        # preferences lock, then rendered and pushed with the same live eval a
        # single slider takes. The prefix *is* the list -- no key names appear in
        # the implementation -- so the digest here is the claim that all seven
        # departures below leave and nothing else does.
        pre_state=stamped(
            '[appearance]\ntheme_mode = "dark"\nglass_mode = "frosted"\n'
            'glass_blur = "heavy"\nglass_transparency = 0.2\n'
            'glass_edge_width = 40\nglass_refraction = 0.2\n'
            'glass_clarity = 0.4\nglass_highlight = 0.4\n'
            'accent_color = "teal"\n'
        ),
    ),
    Scenario(
        name="action-audio-output-volume",
        argv=("action", "audio.output.volume", "0.35"),
        # `str(float(value))` is the whole of what wpctl is handed, and the
        # default sink is addressed by alias rather than by name so a device
        # swapped between the read and the write still moves the right slider.
    ),
    Scenario(
        name="action-audio-output-volume-integer",
        argv=("action", "audio.output.volume", "1"),
        # The case a port gets wrong in the safe-looking direction: JSON `1` is a
        # Python `float` by the time wpctl sees it, so the argument is `1.0` and
        # not `1`. Rust's own `Display` for `f64` writes `1`.
    ),
    Scenario(
        name="action-audio-input-volume",
        argv=("action", "audio.input.volume", "0.5"),
    ),
    Scenario(
        name="action-audio-output-mute",
        argv=("action", "audio.output.mute", "true"),
        # `"1" if value else "0"`: Python truthiness, not JSON's.
    ),
    Scenario(
        name="action-audio-input-mute-falsy",
        argv=("action", "audio.input.mute", "0"),
        # The falsy half of that truthiness, spelled as a number rather than as
        # `false`, which is what the pane's own binding sends for "unmuted".
    ),
    Scenario(
        name="action-audio-output-default",
        argv=("action", "audio.output.default", '"alsa_output.usb-Generic_USB_Audio-00.analog-stereo"'),
        # The one audio concept `wpctl` has no verb for, so it goes to `pactl`.
    ),
    Scenario(
        name="action-audio-input-default",
        argv=("action", "audio.input.default", '"alsa_input.pci-0000_00_1f.3.analog-stereo"'),
    ),
    Scenario(
        name="action-audio-volume-not-a-number",
        argv=("action", "audio.output.volume", '"loud"'),
        # `float()`'s own ValueError, through the envelope, with nothing reaching
        # wpctl. The wording is CPython's because that is what the pane shows.
    ),
    Scenario(
        name="action-defaults-browser",
        argv=("action", "defaults.browser", '"firefox.desktop"'),
        fixtures="session",
        pre_state=dict(APPLICATIONS),
        # The role that does more than write mimeapps.list: the browser marker is
        # republished from the freshly resolved association and the compositor is
        # reloaded, because binds.lua reads that marker the way it reads the
        # terminal one. Three `gio mime` calls, then the marker, then the reload.
    ),
    Scenario(
        name="action-defaults-unknown-role",
        argv=("action", "defaults.spreadsheet", '"gnumeric.desktop"'),
        # `Unknown default application: spreadsheet`, before anything is read.
    ),
    Scenario(
        name="action-lock-now",
        argv=("action", "lock.now"),
        # One call to logind, checked: a lock that did not happen is worth saying.
    ),
    Scenario(
        name="action-datetime-ntp",
        argv=("action", "datetime.ntp", "true"),
        # Spawned detached rather than run to completion: nothing reads a result
        # and systemd-timedated does the work over the bus, so waiting would stall
        # the settings path for an answer nobody uses. The shim records the exec
        # either way, which is what makes the two backends comparable here.
    ),
    Scenario(
        name="action-datetime-timezone",
        argv=("action", "datetime.timezone", '"Europe/Amsterdam"'),
        fixtures="session",
        # Checked against `timedatectl list-timezones` every time rather than
        # cached: a tzdata update between the pane populating its picker and the
        # user choosing is exactly the case the check exists for.
    ),
    Scenario(
        name="action-datetime-timezone-unknown",
        argv=("action", "datetime.timezone", '"Mars/Olympus"'),
        fixtures="session",
        # `Unknown timezone`, and nothing spawned.
    ),
    Scenario(
        name="action-datetime-timezone-no-timedatectl",
        argv=("action", "datetime.timezone", '"Europe/Amsterdam"'),
        # No fixture, so `timedatectl list-timezones` answers nothing and *every*
        # timezone is unknown. That is the Python's behaviour as written, and the
        # safe direction to fail in -- pinned so a port cannot "improve" it into
        # accepting anything when the list cannot be read.
    ),
    Scenario(
        name="action-keybind-rebind-unpublished",
        argv=("action", "keybind.rebind", '{"id": "super+t", "keys": "SUPER+ALT+T"}'),
        # The catalog fragment has never been published on this scratch machine,
        # so the change is refused with the sentence that says why rather than
        # with "there is no shortcut called super+t". The fail-closed half of the
        # catalog contract, reached through `action` rather than in-process.
    ),
    Scenario(
        name="action-unknown",
        argv=("action", "nope.nothing"),
        # The final `else`: `Unknown action: nope.nothing`.
    ),
    Scenario(
        name="action-no-name",
        argv=("action",),
        # `argv[2]` with nothing there. The Python raises IndexError, which
        # main()'s except tuple does not catch -- a traceback on stderr and exit 1
        # -- where the port answers through the envelope. Written down in
        # deviations.toml; the exit status is 1 either way.
    ),
)


# ---------------------------------------------------------------------------
# snapshot: `garage snapshot`, and the default with no command at all
# ---------------------------------------------------------------------------
# The heaviest shim traffic in the corpus: monitors, devices, pipewire, pulse,
# timedatectl, locale, mime lookups. Almost all of the work is in the trace, which
# is exactly the surface a port is most likely to get quietly wrong -- and the
# stdout is one JSON object whose key order is the Python dict's insertion order,
# so the envelope is compared byte for byte.

SNAPSHOT: tuple[Scenario, ...] = (
    Scenario(
        name="snapshot-empty",
        argv=("snapshot",),
        pre_state=dict(APPLICATIONS),
        # A machine with no preferences.toml, no displays.toml and no keybindings:
        # every section is answered from the fixtures alone.
    ),
    Scenario(
        name="snapshot-default-command",
        argv=(),
        fixtures="snapshot-empty",
        pre_state=dict(APPLICATIONS),
        # No subcommand at all, which `main()` resolves to `snapshot`. The same
        # answer as above, which is the point: this pins the default, and the
        # QML client's simplest possible invocation is exactly this one.
    ),
    Scenario(
        name="snapshot-degraded",
        argv=("snapshot",),
        fixtures="snapshot-empty",
        pre_state=dict(APPLICATIONS) | prefs("this is not toml\n"),
        # The degrade path: preferences that cannot be read fall back to a deep
        # copy of the shipped defaults and the refusal is carried in `error`, so
        # the pane comes up able to say what is wrong instead of not coming up.
    ),
    Scenario(
        name="snapshot-with-a-saved-layout",
        argv=("snapshot",),
        fixtures="snapshot-empty",
        pre_state=dict(APPLICATIONS) | {
            ".config/garage/displays.toml":
                'primary = "DP-2"\n\n[[display]]\noutput = "DP-2"\n'
                'description = "Dell Inc. DELL U2720Q (DP-2)"\nenabled = true\n'
                'mode = "3840x2160@59.9"\nx = 0\ny = 0\nscale = 1.5\n'
                'transform = 0\nvrr = 2\n',
        },
        # The fold between what the compositor reports and what displays.toml
        # remembers: the saved `mode` and the saved `vrr` outrank the live ones,
        # and the saved primary outranks $HYPR_PRIMARY_MONITOR. Also the case
        # where workspaces_snapshot() sees a display for the first time and
        # writes workspace-blocks.toml on a read, which the digest records.
    ),
)


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
        active=True,
        note="the harness proving it compares things; not a layer claim. Its two other "
             "cases graduated to `snapshot` and `palette` when task 3.10+3.11 ported the "
             "layers under them.",
        scenarios=SMOKE,
    ),
    Family(name="preferences", active=True,
           note="load/validate/save and the whole `set` argv surface; from "
                "test_preferences.py, test_schema.py. The load half is task 3.1's "
                "(garage-prefs) and the schema is task 3.2's; task 3.10+3.11 made the "
                "route walk complete, which is what let the `set` half be claimed. Active "
                "with five written-down deviations, every one of them a consequence of the "
                "schema being *typed* here and a dict there -- see deviations.toml.",
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
    Family(name="palette", active=True,
           note="theme-sync, night-shift-sync, the accent push and the whole wallpaper "
                "resolver; task 3.10+3.11 ported push_theme(), apply_wallpaper(), "
                "wallpaper_target() and solid_wallpaper(). From test_palette.py and "
                "test_wallpapers.py.",
           scenarios=PALETTE),
    Family(name="apply", active=True,
           note="`garage apply` and every route in PREFERENCE_ROUTES that ends in a signal; "
                "task 3.10+3.11 ported the apply layer. Almost purely a trace-surface "
                "family. From test_session_surfaces.py.",
           scenarios=APPLY),
    Family(name="displays", active=True,
           note="display test/confirm/revert and the fifteen-second watchdog; task 3.8 "
                "ported load_display_config(), mirror_targets() and render_displays() into "
                "garage-render and the transaction, the geometry check, the snapshot and the "
                "seeding into garage-apply, and wired all four commands in the CLI.",
           scenarios=DISPLAYS),
    Family(name="files", active=True,
           note="default applications, the mime override and the three general markers; "
                "task 3.10+3.11 wired apply_terminal() and set_default_app() to the CLI. "
                "From test_files.py, test_file_index.py, test_launcher.py.",
           scenarios=FILES),
    Family(name="actions", active=True,
           note="`garage action`, the second dispatch table; task 3.10+3.11 ported action() "
                "and toggle_boolean_preference(). From test_files.py and test_keybinds.py.",
           scenarios=ACTIONS),
    Family(name="snapshot", active=True,
           note="`garage snapshot` and the bare-argv default; task 3.10+3.11 ported "
                "make_snapshot() and its eight live reads. From test_schema.py and "
                "test_files.py.",
           scenarios=SNAPSHOT),
    Family(name="doctor", active=True,
           note="`garage doctor`, `garage repair` and `garage update`; tasks 3.12-3.14 "
                "ported all three and wired them to the CLI's plain-command arms. Active "
                "with two written-down deviations, both of them the same deliberate "
                "departure: the Rust doctor reads system/manifest/*.list at runtime where "
                "the Python names its own DOCTOR_* tuples, and the file is the wider of "
                "the two. From test_recovery.py, test_manifest.py, test_docs.py.",
           scenarios=DOCTOR),
)
