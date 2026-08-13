"""What a run *left behind*, reduced to something two backends can be compared on.

Why a recursive digest and not a handful of assertions about named files: the
Python backend is a config generator, and the thing a port has to reproduce is
not one file's contents but the entire shape of $HOME afterwards -- which files
exist, which are symlinks and to what, and what mode they carry. A test that
checks preferences.toml by name passes happily while the Rust port forgets to
create ~/.local/state/garage/generated/ at all. So every path under the scratch
HOME is walked and reduced to (relpath, kind, mode, symlink target, sha256), and
the whole sorted list is one comparison surface.

Symlinks are recorded, never followed. That is not tidiness -- the scratch HOME
contains a stow-shaped symlink at ~/.config/garage/preferences.defaults.toml
pointing back into this repository, and following it would (a) walk the repo into
the digest and (b) hide the very distinction the digest exists to catch: a
backend that replaces the link with a regular file has broken stow, and the only
evidence is that the entry stopped being a symlink.

Inode identity is the other half, and it is invariant 1 made testable. The Python
backend distinguishes atomic_write() (rename a temporary over the target -- new
inode, correct for config files) from write_marker() (truncate in place -- same
inode, required because Quickshell holds an inotify watch on the marker's inode
and a rename is invisible to it). Contents alone cannot tell those two apart:
both leave identical bytes on disk. So pre-planted files have their inode
recorded before the run, and afterwards each one reports SURVIVED or CHANGED. A
Rust port that "correctly" rewrites every marker atomically produces a
byte-identical digest and a completely different inode report, and the desktop
would stop repainting. That difference has to be visible.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import stat


CHUNK = 1 << 16
LARGE_FILE = 8 << 20


def walk(root: Path):
    """Every path under `root`, symlinks as leaves, in sorted order.

    os.walk is not used: it follows nothing by default but still stats through
    symlinked directories when deciding what a directory is, and the ordering it
    yields is filesystem order. Deterministic output is the whole point here.
    """
    root = Path(root)
    if not root.exists() and not root.is_symlink():
        return
    stack = [root]
    while stack:
        current = stack.pop()
        try:
            entries = sorted(os.scandir(current), key=lambda entry: entry.name)
        except OSError:
            continue
        for entry in entries:
            path = Path(entry.path)
            yield path
            # is_dir(follow_symlinks=False): a symlink to a directory is a leaf.
            # Descending it would digest whatever it points at, which for the
            # defaults link is this repository.
            if entry.is_dir(follow_symlinks=False):
                stack.append(path)


def file_hash(path: Path, normalize=None) -> str:
    """sha256 of the file, with `normalize` applied to its text first.

    Normalizing before hashing rather than after is not an optimisation, it is
    required: several generated files embed the absolute $HOME (qt6ct.conf names
    its colour scheme by full path), and the scratch $HOME is a fresh temporary
    directory on every run. Hashing the raw bytes would make two runs of the
    *same* backend disagree, and the determinism check would report a set
    iteration that does not exist.
    """
    digest = hashlib.sha256()
    try:
        if normalize is None:
            with path.open("rb") as handle:
                while chunk := handle.read(CHUNK):
                    digest.update(chunk)
            return digest.hexdigest()
        # Whole-file for the normalized path: a replacement can straddle any
        # chunk boundary, so this cannot be streamed. The files a desktop
        # config generator writes are kilobytes; the guard below is for the
        # wallpaper-sized surprise, which gets hashed raw instead.
        if path.stat().st_size > LARGE_FILE:
            with path.open("rb") as handle:
                while chunk := handle.read(CHUNK):
                    digest.update(chunk)
            return digest.hexdigest()
        raw = path.read_bytes()
    except OSError as error:
        return f"unreadable:{type(error).__name__}"
    text = normalize(raw.decode("utf-8", "surrogateescape"))
    digest.update(text.encode("utf-8", "surrogateescape"))
    return digest.hexdigest()


def kind_of(path: Path) -> str:
    if path.is_symlink():
        return "symlink"
    try:
        mode = path.stat(follow_symlinks=False).st_mode
    except OSError:
        return "missing"
    if stat.S_ISDIR(mode):
        return "dir"
    if stat.S_ISREG(mode):
        return "file"
    if stat.S_ISFIFO(mode):
        return "fifo"
    if stat.S_ISSOCK(mode):
        return "socket"
    return "other"


def entry_for(root: Path, path: Path, normalize=None) -> dict:
    """One digest row. `target` and `sha256` are empty unless they apply."""
    kind = kind_of(path)
    try:
        mode = stat.S_IMODE(path.lstat().st_mode)
    except OSError:
        mode = 0
    scrub = normalize or (lambda text: text)
    row = {
        "path": scrub(path.relative_to(root).as_posix()),
        "kind": kind,
        "mode": f"{mode:04o}",
        "target": "",
        "sha256": "",
    }
    if kind == "symlink":
        try:
            row["target"] = scrub(os.readlink(path))
        except OSError:
            row["target"] = "<unreadable>"
    elif kind == "file":
        row["sha256"] = file_hash(path, normalize)
    return row


def digest_tree(root: Path, normalize=None) -> tuple[dict, ...]:
    """The whole scratch HOME as a sorted, comparable table.

    `normalize` is the runner's path scrubber. It is applied to relative paths,
    to symlink targets and to file *contents* -- see file_hash for why the last
    one is not optional.
    """
    root = Path(root)
    rows = [entry_for(root, path, normalize) for path in walk(root)]
    rows.sort(key=lambda row: row["path"])
    return tuple(rows)


def format_digest(rows) -> str:
    """The digest as text, one row per line, for a readable diff."""
    lines = []
    for row in rows:
        detail = row["target"] and f" -> {row['target']}" or ""
        checksum = row["sha256"] and f" {row['sha256'][:16]}" or ""
        lines.append(f"{row['kind']:8} {row['mode']} {row['path']}{detail}{checksum}")
    return "\n".join(lines)


@dataclass(frozen=True)
class InodeSnapshot:
    """Inode numbers for the files a scenario planted, taken before the run."""

    numbers: dict[str, int | None]

    def verdicts(self, root: Path) -> tuple[str, ...]:
        """SURVIVED / CHANGED / GONE / CREATED, one per pre-planted path.

        SURVIVED means the file was edited in place (write_marker semantics).
        CHANGED means something replaced it -- a rename, a delete-and-recreate,
        or an unlink of a symlink followed by a fresh file. Both are legitimate
        somewhere in the backend, and which one happened where *is* the
        behaviour under test, so the verdict is reported rather than asserted.
        """
        root = Path(root)
        report = []
        for relpath in sorted(self.numbers):
            before = self.numbers[relpath]
            try:
                after = (root / relpath).lstat().st_ino
            except OSError:
                after = None
            if before is None and after is None:
                verdict = "ABSENT"
            elif before is None:
                verdict = "CREATED"
            elif after is None:
                verdict = "GONE"
            elif before == after:
                verdict = "SURVIVED"
            else:
                verdict = "CHANGED"
            report.append(f"{verdict} {relpath}")
        return tuple(report)


def snapshot_inodes(root: Path, relpaths) -> InodeSnapshot:
    """Record inode numbers for `relpaths` under `root`, before anything runs."""
    root = Path(root)
    numbers: dict[str, int | None] = {}
    for relpath in relpaths:
        try:
            numbers[relpath] = (root / relpath).lstat().st_ino
        except OSError:
            numbers[relpath] = None
    return InodeSnapshot(numbers)
