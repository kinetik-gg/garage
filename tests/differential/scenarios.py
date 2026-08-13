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
# smoke: the harness proving itself
# ---------------------------------------------------------------------------
# Not a layer. Five invocations chosen because between them they touch every
# comparison surface the runner has: a pure-stdout command, an error path, a
# file-writing command with no external calls, a command that is almost entirely
# external calls, and a command that reads, validates, writes and signals.

SMOKE = (
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
        # falls back to.
    ),
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

RENDER: tuple[Scenario, ...] = ()
# Grown from tests/test_bar.py, tests/test_keybinds.py and
# tests/test_schema_table.py: render, render-bar, render-wallpaper and the Lua
# fragments -- the family the luac shim exists for.

PALETTE: tuple[Scenario, ...] = ()
# Grown from tests/test_palette.py and tests/test_wallpapers.py: theme-sync,
# night-shift-sync, wallpaper selection, and the marker files whose *inodes*
# Quickshell watches.

APPLY: tuple[Scenario, ...] = ()
# Grown from tests/test_session_surfaces.py: apply, and every gsettings /
# hyprctl / systemctl call it makes. Almost purely a trace-surface family.

DISPLAYS: tuple[Scenario, ...] = ()
# Grown from tests/test_recovery.py: display-test, display-confirm,
# display-revert and the watchdog, plus displays.toml round-tripping.

FILES: tuple[Scenario, ...] = ()
# Grown from tests/test_files.py, tests/test_file_index.py and
# tests/test_launcher.py: the default-application roles and the mime writes.

DOCTOR: tuple[Scenario, ...] = ()
# Grown from tests/test_manifest.py and tests/test_docs.py: doctor, repair and
# update -- the three commands that print human lines instead of the JSON
# envelope, so their stdout is compared verbatim.


FAMILIES = (
    Family(
        name="smoke",
        active=False,
        note="the harness proving it compares things; not a layer claim",
        scenarios=SMOKE,
    ),
    Family(name="preferences", active=False,
           note="load/validate/save; from test_preferences.py, test_schema.py. "
                "The load half is ported (task 3.1, garage-prefs) but reaches the "
                "CLI through render-idle, which is still a stub; the `set` half "
                "needs main()'s set branch, task 3.15. Activated by task 3.2.",
           scenarios=PREFERENCES),
    Family(name="render", active=False,
           note="fragment generation; from test_bar.py, test_keybinds.py",
           scenarios=RENDER),
    Family(name="palette", active=False,
           note="theme and wallpaper markers; from test_palette.py",
           scenarios=PALETTE),
    Family(name="apply", active=False,
           note="session signalling; from test_session_surfaces.py",
           scenarios=APPLY),
    Family(name="displays", active=False,
           note="display test/confirm/revert; from test_recovery.py",
           scenarios=DISPLAYS),
    Family(name="files", active=False,
           note="default applications and mime; from test_files.py",
           scenarios=FILES),
    Family(name="doctor", active=False,
           note="human-line commands; from test_manifest.py, test_docs.py",
           scenarios=DOCTOR),
)
