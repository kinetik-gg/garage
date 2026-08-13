"""Spawn both backends against the same scratch world and compare what happened.

Why subprocesses and not tests/harness.py's in-process load: the thing being
verified is a *port*, and only one of the two implementations is importable
Python. The comparison therefore has to happen at the process boundary -- argv
in, stdout/stderr/exit-status out, plus everything the process did to the
filesystem and to the commands it spawned. tests/harness.py stays exactly as it
is and keeps doing its own job; nothing here imports the backend.

Why fixtures and not record/replay. A recorder would capture this machine: the
monitors actually attached, the pipewire graph actually running, whichever
timezone list this /usr/share/zoneinfo happens to ship. Re-running the suite next
month on a laptop with one display would then "fail" a port that is perfectly
correct. Worse, a recording is only ever of the Python side, so the Rust run gets
replayed answers to questions it may legitimately ask in a different order --
which silently converts a trace difference (a real finding) into fabricated
input. Fixtures are written by hand, per scenario, as a *specification of the
world*: both backends are asked the same questions of the same fake machine, and
if they ask different questions the trace says so out loud instead of papering
over it.

Why the trace is a first-class surface. Half of what `garage` does is not on
stdout at all -- it is `hyprctl reload`, `systemctl --user restart
hypridle.service`, `gsettings set ... color-scheme`. Those calls, in order, with
their exact arguments, are the observable behaviour of the desktop changing. A
port that generates the right files and never signals anything leaves the session
looking untouched until the next login. So PATH is clamped to the shim directory
and nothing else: any exec either backend performs resolves to a shim, gets
recorded, and gets a scripted answer. Both binaries are invoked by absolute path
precisely so that clamp can be total.

Determinism is checked before parity and independently of it. Every Python case
runs twice, under PYTHONHASHSEED=1 and PYTHONHASHSEED=2. If the two disagree on
any surface, that is a bug in the *Python* backend -- a set iteration that
reached an output -- and it is reported as a loud failure rather than as a Rust
mismatch, because no port can be expected to match a moving target.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
import json
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

from . import digest as digest_module


TESTS_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = TESTS_DIR.parent
BACKEND_PATH = REPO_ROOT / "desktop" / ".local" / "bin" / "garage"
DEFAULTS_SOURCE = REPO_ROOT / "desktop" / ".config" / "garage" / "preferences.defaults.toml"
RUST_DIR = REPO_ROOT / "backend"
RUST_BINARY = RUST_DIR / "target" / "debug" / "garage"
DIFFERENTIAL_DIR = Path(__file__).resolve().parent
SHIM_TEMPLATES = DIFFERENTIAL_DIR / "shims"
FIXTURES_DIR = DIFFERENTIAL_DIR / "fixtures"
DEVIATIONS_PATH = DIFFERENTIAL_DIR / "deviations.toml"

# Every external binary the backend can reach, from a sweep of its run(),
# subprocess.Popen() and shutil.which() call sites. `env` is on the list because
# the mime lookup goes through `env LC_ALL=C gio mime ...`; `gio` is on it too so
# that a direct call could never escape to the real one. `sh` is deliberately
# absent -- the backend never shells out, and a shim for it would be a shim for
# something that would be a bug if it were ever invoked.
SHIM_NAMES = (
    "Hyprland",
    "dbus-update-activation-environment",
    "env",
    "fc-list",
    "file",
    "gio",
    "git",
    "gsettings",
    "hyprctl",
    "locale",
    "loginctl",
    "magick",
    "pacman",
    "pactl",
    "pkill",
    "systemctl",
    "timedatectl",
    "wpctl",
)

# The order a report reads best in: cheapest signal first, then the two that say
# what the run actually did.
SURFACES = ("exit_code", "stdout", "stderr", "trace", "digest", "inodes")

CASE_TIMEOUT = 120
BUILD_TIMEOUT = 600

# The staging suffix on a hidden dotfile -- `.preferences.lua.XXXXXXXX` in the
# trace (`luac -p .../.preferences.lua.k3f9x_2q`). Two unrelated generators
# produce it, and neither is a difference between the two backends: Python's
# `tempfile.mkstemp()` is eight characters from `[a-z0-9_]`, and the Rust port's
# `garage_core::fs::atomic` writer concatenates pid/nanosecond/counter as lower-
# case hex, which is not fixed-width and commonly runs past eight characters
# (`2b2fc729773c2600`). {6,32} covers both generators' real output without
# reaching into an ordinary short extension -- nothing this backend writes ships
# a *hidden* two-dot filename whose last segment is six to thirty-two lowercase
# alphanumerics/underscores for any other reason.
_MKSTEMP = re.compile(r"(?<=/)\.([^/\s]+)\.([a-z0-9_]{6,32})\b")

# A display transaction's token: `uuid.uuid4().hex` there, sixteen bytes of /dev/urandom
# spelled the same way here. It is a nonce, so it differs between every run of either
# backend -- including the two Python runs the determinism check compares -- and it reaches
# stdout (the envelope's `data.token`), the pending record and the watchdog's argv. Scrubbed
# rather than seeded: neither backend offers a way to fix it, and pinning it would test a
# hook that does not exist in production.
_TOKEN = re.compile(r"\b[0-9a-f]{32}\b")


class HarnessError(RuntimeError):
    """The harness itself could not run a case. Never a parity verdict."""


class NondeterminismError(AssertionError):
    """Two runs of the *Python* backend disagreed with each other."""


@dataclass(frozen=True)
class Scenario:
    """One comparable invocation: argv, the world it runs against, the state it starts from.

    `then` makes it two invocations against *one* world, which the display transaction needs
    and nothing else does: `display-test` applies a layout and returns a token, and only a
    second process carrying that token can confirm or revert it. Splitting the pair into two
    scenarios would compare the halves against two different scratch worlds and never
    exercise the thing the transaction *is*. A lone `display-test` is not an option either --
    it leaves a pending record holding a fresh uuid and a wall-clock `expires`, which is a
    determinism failure rather than a comparison.

    The literal `$TOKEN` anywhere in `then` is replaced by the token the first command printed
    in its envelope's `data.token`, so the second step carries whatever the backend under test
    generated rather than a value the harness invented.
    """

    name: str
    argv: tuple[str, ...]
    #: relpath under scratch HOME -> file contents, or ("symlink", target).
    pre_state: dict[str, object] = field(default_factory=dict)
    #: GARAGE_* / XDG overrides set for this case only, on top of the scratch base.
    env: dict[str, str] = field(default_factory=dict)
    #: Name of the JSON table in fixtures/. Defaults to the scenario name.
    fixtures: str | None = None
    #: A second argv run in the same world, with `$TOKEN` substituted. See the class doc.
    then: tuple[str, ...] = ()

    @property
    def fixture_path(self) -> Path:
        return FIXTURES_DIR / f"{self.fixtures or self.name}.json"

    def steps(self) -> tuple[tuple[str, ...], ...]:
        return (self.argv, self.then) if self.then else (self.argv,)


@dataclass(frozen=True)
class Family:
    """A group of scenarios that one Phase 3 layer task claims parity for."""

    name: str
    #: False until the layer lands. Inactive families still run; mismatches skip.
    active: bool
    note: str
    scenarios: tuple[Scenario, ...] = ()


@dataclass(frozen=True)
class Capture:
    """Everything one process did, normalized and ready to compare."""

    #: One status per step, in order -- a single-step scenario is a one-tuple, and its
    #: surface text is what it always was.
    returncode: tuple[int, ...]
    stdout: bytes
    stderr: bytes
    trace: tuple[str, ...]
    digest: tuple[dict, ...]
    inodes: tuple[str, ...]

    def surface(self, name: str) -> str:
        """One comparison surface as printable text."""
        if name == "exit_code":
            return " ".join(str(code) for code in self.returncode)
        if name == "stdout":
            return self.stdout.decode("utf-8", "replace")
        if name == "stderr":
            return self.stderr.decode("utf-8", "replace")
        if name == "trace":
            return "\n".join(self.trace)
        if name == "digest":
            return digest_module.format_digest(self.digest)
        if name == "inodes":
            return "\n".join(self.inodes)
        raise HarnessError(f"unknown surface: {name}")


@dataclass(frozen=True)
class Deviation:
    """A difference that has been looked at, understood and written down."""

    surface: str
    scenario: str
    reason: str


@dataclass(frozen=True)
class Result:
    """The verdict for one scenario, with both captures kept for the report."""

    scenario: Scenario
    python: Capture
    rust: Capture
    differing: tuple[str, ...]
    forgiven: tuple[str, ...]
    unforgiven: tuple[str, ...]

    def summary(self) -> str:
        if not self.differing:
            return f"{self.scenario.name}: identical"
        parts = ", ".join(self.differing)
        return f"{self.scenario.name}: {parts}"

    def report(self) -> str:
        """A full side-by-side for the surfaces that differ."""
        blocks = [f"scenario {self.scenario.name}  argv={list(self.scenario.argv)}"]
        for surface in self.unforgiven or self.differing:
            blocks.append(
                f"--- {surface}\n"
                f"  python: {self.python.surface(surface)!r}\n"
                f"  rust:   {self.rust.surface(surface)!r}")
        return "\n".join(blocks)


# ---------------------------------------------------------------------------
# setup: binaries and shims
# ---------------------------------------------------------------------------

def build_rust(timeout: int = BUILD_TIMEOUT) -> Path:
    """`cargo build -p garage-cli`, then hand back the binary's absolute path."""
    try:
        completed = subprocess.run(
            ["cargo", "build", "-p", "garage-cli"],
            cwd=str(RUST_DIR), capture_output=True, text=True,
            check=False, timeout=timeout)
    except FileNotFoundError as error:
        raise HarnessError(f"cargo is not installed: {error}") from error
    except subprocess.TimeoutExpired as error:
        raise HarnessError(f"cargo build exceeded {timeout}s") from error
    if completed.returncode != 0:
        raise HarnessError(
            "cargo build -p garage-cli failed:\n"
            f"{completed.stdout}\n{completed.stderr}")
    if not RUST_BINARY.exists():
        raise HarnessError(f"cargo build succeeded but {RUST_BINARY} is missing")
    return RUST_BINARY


def materialize_shims(destination: Path, interpreter: str, luac: str | None) -> tuple[str, ...]:
    """Write the executable shims into `destination`; return the names written.

    The interpreter is baked in as an absolute shebang because the child's PATH
    is this directory alone. `luac` is written only if a real one exists, so that
    `shutil.which("luac")` answers truthfully in both backends.
    """
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    body = (SHIM_TEMPLATES / "fixture_shim.py").read_text(encoding="utf-8")
    written = []
    for name in SHIM_NAMES:
        path = destination / name
        path.write_text(f"#!{interpreter}\n{body}", encoding="utf-8")
        path.chmod(0o755)
        written.append(name)
    if luac:
        body = (SHIM_TEMPLATES / "luac_shim.py").read_text(encoding="utf-8")
        path = destination / "luac"
        path.write_text(f"#!{interpreter}\n{body.replace('@LUAC@', luac)}",
                        encoding="utf-8")
        path.chmod(0o755)
        written.append("luac")
    return tuple(sorted(written))


def find_luac() -> str | None:
    """The real luac, resolved before any PATH is clamped."""
    return shutil.which("luac")


# ---------------------------------------------------------------------------
# per-case scratch world
# ---------------------------------------------------------------------------

def plant(home: Path, pre_state: dict) -> None:
    """Create a scenario's starting files under the scratch HOME."""
    for relpath, content in pre_state.items():
        target = home / relpath
        target.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, tuple) and content and content[0] == "symlink":
            target.symlink_to(content[1])
        else:
            target.write_text(str(content), encoding="utf-8")


def prepare_home(root: Path, scenario: Scenario) -> Path:
    """A fresh $HOME with the production-shaped defaults symlink in place.

    The defaults file is a *symlink into this repository*, not a copy, because
    that is its shape on a stowed machine and because it is exactly what
    write_marker()'s stow protection exists to survive. A copy would let a
    backend that follows and truncates the link look correct here and edit the
    developer's checkout in the field.
    """
    home = Path(root) / "home"
    (home / ".config" / "garage").mkdir(parents=True, exist_ok=True)
    (home / ".local" / "state" / "garage").mkdir(parents=True, exist_ok=True)
    (home / ".local" / "share").mkdir(parents=True, exist_ok=True)
    (home / ".cache").mkdir(parents=True, exist_ok=True)
    link = home / ".config" / "garage" / "preferences.defaults.toml"
    if not link.exists() and not link.is_symlink():
        link.symlink_to(DEFAULTS_SOURCE)
    plant(home, scenario.pre_state)
    return home


def case_environment(home: Path, shim_dir: Path, trace: Path, fixtures: Path,
                     scenario: Scenario, hashseed: str) -> dict[str, str]:
    """The child's entire environment, built from nothing rather than inherited.

    Built from nothing on purpose: a GARAGE_PREFERENCES left over in the
    developer's shell would point one of the two runs at a different file and the
    harness would report a port bug that is really a laptop. XDG_DATA_HOME and
    XDG_DATA_DIRS are clamped into the scratch too -- the default-applications
    snapshot scans <data dir>/applications, and without the clamp the answer is
    "whatever this machine has installed", which is neither reproducible nor the
    same question on another machine.
    """
    environment = {
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_STATE_HOME": str(home / ".local" / "state"),
        "XDG_DATA_HOME": str(home / ".local" / "share"),
        "XDG_DATA_DIRS": str(home / ".local" / "share"),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "XDG_RUNTIME_DIR": str(home / ".cache"),
        "PATH": str(shim_dir),
        "LANG": "en_US.UTF-8",
        "LC_ALL": "C",
        "TZ": "UTC",
        "PYTHONHASHSEED": hashseed,
        "PYTHONDONTWRITEBYTECODE": "1",
        "GARAGE_DIFF_TRACE": str(trace),
        "GARAGE_DIFF_FIXTURES": str(fixtures),
    }
    environment.update(scenario.env)
    return environment


def normalizer(root: Path, home: Path, shim_dir: Path):
    """Replace the machine-specific paths with stable placeholders.

    Everything here is a path that changes on every run or on every machine, and
    none of it is a difference between the two backends. Leaving it in would make
    the Python-vs-Python determinism check fail on the temp directory name.
    """
    replacements = [
        (str(shim_dir), "$SHIMS"),
        (str(home), "$HOME"),
        (str(root), "$ROOT"),
        (str(REPO_ROOT), "$REPO"),
        (sys.executable, "$PYTHON"),
    ]
    # Longest first: $HOME lives under $ROOT and would otherwise never match.
    replacements.sort(key=lambda pair: len(pair[0]), reverse=True)

    def normalize(text: str) -> str:
        for needle, placeholder in replacements:
            text = text.replace(needle, placeholder)
        return _TOKEN.sub("$TOKEN", _MKSTEMP.sub(r".\1.XXXXXXXX", text))

    return normalize


def execute(command: list[str], scenario: Scenario, shim_dir: Path,
            hashseed: str = "1") -> Capture:
    """Run one binary in a brand-new scratch world and capture every surface."""
    with tempfile.TemporaryDirectory(prefix="garage-diff-") as name:
        root = Path(name)
        home = prepare_home(root, scenario)
        trace = root / "trace"
        trace.write_text("", encoding="utf-8")
        fixtures = root / "fixtures.json"
        source = scenario.fixture_path
        fixtures.write_text(
            source.read_text(encoding="utf-8") if source.exists() else "{}",
            encoding="utf-8")
        before = digest_module.snapshot_inodes(home, sorted(scenario.pre_state))
        environment = case_environment(home, shim_dir, trace, fixtures,
                                       scenario, hashseed)
        statuses, out, err = [], b"", b""
        token = ""
        for step in scenario.steps():
            argv = [token if part == "$TOKEN" else part for part in step]
            try:
                completed = subprocess.run(
                    [*command, *argv], cwd=str(home), env=environment,
                    capture_output=True, check=False, timeout=CASE_TIMEOUT)
            except subprocess.TimeoutExpired as error:
                raise HarnessError(
                    f"{scenario.name}: {command[0]} exceeded {CASE_TIMEOUT}s") from error
            except OSError as error:
                raise HarnessError(
                    f"{scenario.name}: cannot run {command[0]}: {error}") from error
            statuses.append(completed.returncode)
            out += completed.stdout
            err += completed.stderr
            token = token_of(completed.stdout) or token
        normalize = normalizer(root, home, shim_dir)
        rows = digest_module.digest_tree(home, normalize)
        return Capture(
            returncode=tuple(statuses),
            stdout=normalize(out.decode("utf-8", "surrogateescape")
                             ).encode("utf-8", "surrogateescape"),
            stderr=normalize(err.decode("utf-8", "surrogateescape")
                             ).encode("utf-8", "surrogateescape"),
            trace=tuple(normalize(line) for line in
                        trace.read_text(encoding="utf-8").splitlines()),
            digest=rows,
            inodes=before.verdicts(home),
        )


def token_of(stdout: bytes) -> str:
    """The envelope's `data.token`, for a `$TOKEN` in the step that follows.

    Read out of the *unnormalized* stdout, because the token the next command has to present
    is the one the backend generated -- the scrubbed `$TOKEN` placeholder would be refused.
    Anything that is not an envelope carrying one is no token, which is the right answer for
    every scenario that does not have a second step.
    """
    try:
        envelope = json.loads(stdout.decode("utf-8", "surrogateescape"))
    except (ValueError, UnicodeDecodeError):
        return ""
    data = envelope.get("data") if isinstance(envelope, dict) else None
    found = data.get("token") if isinstance(data, dict) else None
    return found if isinstance(found, str) else ""


def python_command(scenario: Scenario) -> list[str]:
    """The Python backend, by absolute path, through this interpreter.

    Not the script's own shebang: `#!/usr/bin/env python3` needs env on PATH,
    and PATH is shims-only by design. argv[0] is still the script path, which is
    what the backend's `argv[1]` dispatch expects.
    """
    return [sys.executable, str(BACKEND_PATH)]


def rust_command(scenario: Scenario) -> list[str]:
    return [str(RUST_BINARY)]


# ---------------------------------------------------------------------------
# comparison
# ---------------------------------------------------------------------------

def compare(python: Capture, rust: Capture) -> dict[str, tuple[str, str]]:
    """The surfaces on which the two captures disagree, as (python, rust) text."""
    differences = {}
    for surface in SURFACES:
        left, right = python.surface(surface), rust.surface(surface)
        if left != right:
            differences[surface] = (left, right)
    return differences


def check_determinism(first: Capture, second: Capture, scenario: Scenario) -> None:
    """Fail loudly if two Python runs under different hash seeds disagree."""
    differences = compare(first, second)
    if not differences:
        return
    detail = "\n".join(
        f"--- {surface}\n  seed 1: {left!r}\n  seed 2: {right!r}"
        for surface, (left, right) in differences.items())
    raise NondeterminismError(
        f"nondeterministic Python output -- a set iteration escaped.\n"
        f"scenario {scenario.name} differed between PYTHONHASHSEED=1 and =2 on "
        f"{', '.join(differences)}:\n{detail}")


def load_deviations(path: Path = DEVIATIONS_PATH) -> tuple[Deviation, ...]:
    """Parse deviations.toml into a tuple of documented differences."""
    if not path.exists():
        return ()
    with path.open("rb") as handle:
        document = tomllib.load(handle)
    entries = document.get("deviation", [])
    deviations = []
    for index, entry in enumerate(entries):
        missing = [key for key in ("surface", "scenario", "reason") if not entry.get(key)]
        if missing:
            raise HarnessError(
                f"deviations.toml entry {index} is missing {', '.join(missing)}; "
                "an undocumented deviation is not a deviation, it is a bug")
        if entry["surface"] not in SURFACES:
            raise HarnessError(
                f"deviations.toml entry {index} names surface {entry['surface']!r}, "
                f"which is not one of {', '.join(SURFACES)}")
        deviations.append(Deviation(entry["surface"], entry["scenario"], entry["reason"]))
    return tuple(deviations)


def forgive(scenario: Scenario, differing, deviations) -> tuple[tuple[str, ...], tuple[str, ...]]:
    """Split differing surfaces into (forgiven, unforgiven) for this scenario."""
    excused = {item.surface for item in deviations if item.scenario == scenario.name}
    forgiven = tuple(surface for surface in differing if surface in excused)
    unforgiven = tuple(surface for surface in differing if surface not in excused)
    return forgiven, unforgiven


def stale_deviations(deviations, observed: dict[str, set[str]]) -> tuple[Deviation, ...]:
    """Deviations whose scenario+surface did not actually differ in this run.

    The XPASS discipline, applied to parity: a written-down difference that has
    stopped happening is a lie in the repository. Either the port caught up and
    the entry should go, or the scenario stopped exercising the thing and the
    entry is protecting nothing. Both are worth a failure.
    """
    stale = []
    for item in deviations:
        if item.scenario not in observed:
            stale.append(item)
        elif item.surface not in observed[item.scenario]:
            stale.append(item)
    return tuple(stale)


def run_scenario(scenario: Scenario, shim_dir: Path,
                 deviations=()) -> Result:
    """Run one case on both backends, twice on Python, and compare.

    Raises NondeterminismError before any parity verdict is reached: a Python
    backend that cannot agree with itself is a finding of its own, and no port
    can be asked to match it.
    """
    first = execute(python_command(scenario), scenario, shim_dir, hashseed="1")
    second = execute(python_command(scenario), scenario, shim_dir, hashseed="2")
    check_determinism(first, second, scenario)
    rust = execute(rust_command(scenario), scenario, shim_dir, hashseed="1")
    differing = tuple(compare(first, rust))
    forgiven, unforgiven = forgive(scenario, differing, deviations)
    return Result(scenario=scenario, python=first, rust=rust,
                  differing=differing, forgiven=forgiven, unforgiven=unforgiven)
