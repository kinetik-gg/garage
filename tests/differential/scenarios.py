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

PREFERENCES: tuple[Scenario, ...] = ()
# Grown from tests/test_preferences.py and tests/test_schema.py: load, merge over
# defaults, validate, migrate, save. The `set`/`action` argv surface and every
# rejection message.

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
           note="load/validate/save; from test_preferences.py, test_schema.py",
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
