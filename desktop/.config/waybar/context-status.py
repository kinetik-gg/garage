#!/usr/bin/env python3
"""Emit quiet, contextual status indicators for Waybar."""

import json
import re
import subprocess
import sys
from pathlib import Path

# These scripts emit their own markup, so the bar's stylesheet cannot reach the
# colours; they resolve the current appearance themselves.
def _theme_fg() -> str:
    try:
        scheme = (Path.home() / ".local/state/garage/generated/color-scheme").read_text().strip()
    except OSError:
        scheme = "dark"
    return "#1c1c1e" if scheme == "light" else "#f5f5f7"


THEME_FG = _theme_fg()




def run(command):
    return subprocess.run(
        command,
        capture_output=True,
        text=True,
        timeout=3,
        check=False,
    ).stdout


def emit(text="", tooltip="", style="idle"):
    print(json.dumps({"text": text, "tooltip": tooltip, "class": style}))


def containers():
    names = set()
    for engine in ("podman", "docker"):
        output = run([engine, "ps", "--format", "{{.Names}}"])
        names.update(name for name in output.splitlines() if name)
    if names:
        emit(f"CTR {len(names)}", "Running containers\n" + "\n".join(sorted(names)), "active")
    else:
        emit()


def microphone():
    output = run(["pactl", "list", "sources"])
    sections = re.split(r"(?=^Source #)", output, flags=re.MULTILINE)
    recording = []
    for section in sections:
        name = re.search(r"^\s*Name:\s*(.+)$", section, flags=re.MULTILINE)
        state = re.search(r"^\s*State:\s*(.+)$", section, flags=re.MULTILINE)
        description = re.search(r"^\s*Description:\s*(.+)$", section, flags=re.MULTILINE)
        if name and state and state.group(1) == "RUNNING" and not name.group(1).endswith(".monitor"):
            recording.append(description.group(1) if description else name.group(1))
    if recording:
        emit(
            'MIC <span foreground="#ffd60ae6">●</span>',
            "Microphone active\n" + "\n".join(recording),
            "recording",
        )
    else:
        emit(f'MIC <span foreground="{THEME_FG}73">●</span>', "Microphone inactive", "idle")


def smb():
    helper = Path.home() / ".local/libexec/ensure-smb-mounted"
    if not helper.exists():
        emit()
        return
    expected = set(re.findall(r'"(smb://[^" ]+/)"', helper.read_text()))
    mounted = run(["gio", "mount", "-l"])
    missing = sorted(share for share in expected if f" -> {share}" not in mounted)
    connected = len(expected) - len(missing)
    if missing:
        labels = [share.rstrip("/").rsplit("/", 1)[-1] for share in missing]
        emit(
            f"SMB {connected}",
            f"Connected {connected} / {len(expected)}\nUnavailable\n" + "\n".join(labels),
            "warning",
        )
    else:
        emit(f"SMB {connected}", f"All {connected} SMB shares connected", "active")


MODES = {"containers": containers, "microphone": microphone, "smb": smb}
try:
    MODES[sys.argv[1]]()
except (IndexError, OSError, subprocess.SubprocessError):
    emit()
