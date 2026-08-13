"""Capture the Rust backend's own behaviour as the suite's post-Python oracle.

Why this exists: tests/differential's job from Phase 3.10 onward has really been
two different questions wearing one harness. Today it is "does Rust match
Python". From Phase 3.5's templating work onward it also has to answer "does
Rust still match what Rust did yesterday" -- and Phase 4 deletes
desktop/.local/bin/garage outright, which deletes the *only* half of runner.py's
comparison that a live differential run can still make. A golden is the other
half, held still: the Rust binary's own output, captured once per scenario and
checked into the repository, so a regression after both events shows up as "the
port stopped matching itself" instead of the suite having nothing left to check
against at all.

Why this script reuses runner.py's execute()/normalize() rather than writing a
second implementation: identical semantics is the whole point, not a nice-to-
have. Two normalizers that quietly drifted -- one scrubbing $TOKEN, one not --
would fail a golden for a reason that has nothing to do with the backend the
golden exists to pin. So capture_scenario() below calls the exact function
test_parity.py's live path calls for the Rust half of a case (runner.execute()
against runner.rust_command()), and differs from a live run only in what happens
next: write the Capture out, rather than diff it against a second one.

Why one file per scenario, sorted keys, indent=2, a trailing newline: a golden
tree is read by a person on a PR diff far more often than it is read by a
machine, and a stable, alphabetical key order turns "the trace grew a line"
into a two-line diff instead of a reshuffled blob. `git diff` on this tree is
meant to be the first thing a reviewer looks at when Phase 3.5 lands.

Run it directly to (re)capture every golden, from the repository root:

    python3 tests/differential/capture_goldens.py

(equivalently `python3 -m tests.differential.capture_goldens`). It also doubles
as a module: test_parity.py's golden mode imports capture_dict(),
capture_scenario() and golden_path() from here so the write side and the check
side can never disagree about what a golden means.
"""

from __future__ import annotations

import json
import shutil
import sys
import tempfile
from pathlib import Path

# capture_goldens.py has to run two different ways: `python3 path/to/this.py`,
# which Python executes with no package context at all, and the relative import
# every other module in this package uses. The former is what "a runnable
# script" means in practice; the latter is what lets test_parity.py share this
# module's exact notion of a golden instead of reimplementing it.
if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from tests.differential import runner
    from tests.differential.scenarios import FAMILIES
else:
    from . import runner
    from .scenarios import FAMILIES


GOLDEN_DIR = Path(__file__).resolve().parent / "golden"


def capture_dict(capture: "runner.Capture") -> dict:
    """A Capture, reshaped into the plain-old-data a golden file stores.

    Every field arrives already normalized -- runner.execute() ran the token/
    path scrubber before handing back the Capture -- so this is a pure reshape,
    not a second pass of cleanup: tuples become lists because json has no tuple,
    and stdout/stderr become text because json has no bytes. `surrogateescape`
    mirrors the encode/decode round trip execute() itself performs, so a byte
    that is not valid UTF-8 -- nothing in this corpus produces one today, but
    the harness does not get to assume it never will -- survives the trip
    instead of raising.
    """
    return {
        "digest": [dict(row) for row in capture.digest],
        "exit_code": list(capture.returncode),
        "inodes": list(capture.inodes),
        "stderr": capture.stderr.decode("utf-8", "surrogateescape"),
        "stdout": capture.stdout.decode("utf-8", "surrogateescape"),
        "trace": list(capture.trace),
    }


def golden_path(family_name: str, scenario_name: str) -> Path:
    """Where one scenario's golden lives: golden/<family>/<scenario>.json."""
    return GOLDEN_DIR / family_name / f"{scenario_name}.json"


def write_golden(path: Path, data: dict) -> int:
    """Serialize `data` deterministically to `path`; return the bytes written.

    sort_keys is what makes a re-capture's diff empty instead of a reshuffle:
    every surface name and every digest row's keys land in the same order on
    every run, alphabetically, independent of dict insertion order on whatever
    machine did the capturing. ensure_ascii keeps the file plain ASCII even
    though the normalizer has already scrubbed everything machine-specific to a
    placeholder -- a stray non-ASCII byte a fixture round-tripped is still worth
    seeing as \\uXXXX rather than as a raw byte a diff tool renders differently
    depending on locale.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(data, indent=2, sort_keys=True, ensure_ascii=True) + "\n"
    path.write_text(text, encoding="utf-8")
    return len(text.encode("utf-8"))


def capture_scenario(scenario: "runner.Scenario", shim_dir: Path) -> "runner.Capture":
    """Run one scenario against the Rust binary alone -- no Python, no compare.

    hashseed is fixed at "1" only because execute()'s signature wants something
    there; it feeds PYTHONHASHSEED, which the Rust binary never reads and the
    determinism this script cares about (Rust-vs-its-own-golden) does not
    depend on. That is a property of the *port*, not of this harness -- and it
    is exactly what step 4's double-capture check exists to catch if it is ever
    false.
    """
    return runner.execute(runner.rust_command(scenario), scenario, shim_dir, hashseed="1")


def capture_all(shim_dir: Path) -> list[tuple[str, str, int]]:
    """Capture every scenario in every family; return (family, scenario, bytes) rows.

    golden/ is removed first rather than merged into: a scenario that got
    renamed or deleted in scenarios.py has to stop having a golden file too, or
    a stale one would sit there answering a question golden mode no longer asks
    -- silently, since nothing would ever read it again to notice.
    """
    if GOLDEN_DIR.exists():
        shutil.rmtree(GOLDEN_DIR)
    written: list[tuple[str, str, int]] = []
    for family in FAMILIES:
        for scenario in family.scenarios:
            capture = capture_scenario(scenario, shim_dir)
            data = capture_dict(capture)
            path = golden_path(family.name, scenario.name)
            size = write_golden(path, data)
            written.append((family.name, scenario.name, size))
    return written


def main() -> int:
    print("building the Rust binary...", file=sys.stderr)
    runner.build_rust()
    # Resolved before anything clamps PATH; see runner.materialize_shims.
    luac = runner.find_luac()
    with tempfile.TemporaryDirectory(prefix="garage-diff-golden-shims-") as name:
        shim_dir = Path(name)
        runner.materialize_shims(shim_dir, sys.executable, luac)
        written = capture_all(shim_dir)
    total_bytes = sum(size for _, _, size in written)
    print(f"captured {len(written)} scenario goldens, {total_bytes} bytes, "
          f"under {GOLDEN_DIR}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
