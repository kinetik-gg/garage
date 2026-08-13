"""The differential suite's unittest face: one test method per scenario family.

One method per family rather than per scenario because a family is the unit of
claimed parity -- it is what flips from skip to fail when a Phase 3 layer lands.
Individual cases are subTests inside it, so a single broken scenario in an active
family names itself in the report instead of hiding behind a family-level
failure. tests/run's LineReporter prints a line per failing subTest, which is
what makes that readable.

Inactive families still run everything and then skip with the mismatch count in
the reason. That is deliberate: a suite that skipped without running would say
nothing about how far the port has come, and the counts moving toward zero is the
progress signal for the whole sprint.
"""

from __future__ import annotations

from pathlib import Path
import json
import os
import shutil
import sys
import tempfile
import unittest

from . import capture_goldens
from . import runner
from .scenarios import FAMILIES


SHIM_DIR: Path | None = None
DEVIATIONS: tuple[runner.Deviation, ...] = ()
OBSERVED: dict[str, set[str]] = {}
_TEMPORARY: tempfile.TemporaryDirectory | None = None

# Golden mode: compare Rust to the checked-in golden/ tree instead of to a live
# Python run. It switches on by itself once desktop/.local/bin/garage is gone --
# the Phase 4 state this suite has to survive -- and can be switched on early,
# while Python is still here, with GARAGE_DIFF_GOLDEN=1: that is what proves the
# golden tree is already a faithful stand-in before the day it becomes the only
# option. Read once in setUpModule, not at import time, so a test run reflects
# the environment it was actually launched in.
GOLDEN: bool = False


def setUpModule() -> None:
    """Build the Rust binary, materialize the shims, load the deviation list.

    The cargo build happens once for the module, not once per case: it is the
    only slow thing here and it produces the same binary for every scenario.
    """
    global SHIM_DIR, DEVIATIONS, _TEMPORARY, GOLDEN
    runner.build_rust()
    # Resolved before anything clamps PATH; see runner.materialize_shims.
    luac = runner.find_luac()
    _TEMPORARY = tempfile.TemporaryDirectory(prefix="garage-diff-shims-")
    SHIM_DIR = Path(_TEMPORARY.name)
    runner.materialize_shims(SHIM_DIR, sys.executable, luac)
    GOLDEN = (not runner.BACKEND_PATH.is_file()
              or os.environ.get("GARAGE_DIFF_GOLDEN") == "1")
    # deviations.toml documents differences between the two *live* backends;
    # a golden was captured from the Rust side alone, so whatever it embodies
    # -- including every one of those differences -- is already what "correct"
    # means in this mode. Loading the list anyway would only invite something
    # to check it against, which check_family_golden deliberately does not do.
    DEVIATIONS = () if GOLDEN else runner.load_deviations()
    OBSERVED.clear()


def tearDownModule() -> None:
    """Drop the materialized shims.

    The staleness check is a test method, not a teardown: it has to be able to
    fail the run, and an exception raised here would be reported as an error
    against the module rather than as the deliberate verdict it is.
    """
    if _TEMPORARY is not None:
        _TEMPORARY.cleanup()


class RustPortParity(unittest.TestCase):
    """Each method runs one family's corpus against both backends."""

    maxDiff = None

    def check_family(self, family) -> None:
        if not family.scenarios:
            self.skipTest(f"{family.name}: no scenarios yet -- {family.note}")
        if GOLDEN:
            self.check_family_golden(family)
            return
        results = []
        for scenario in family.scenarios:
            # Outside the subTest: a HarnessError or a NondeterminismError is
            # not a parity verdict and must not be swallowed by an inactive
            # family's skip. Both propagate as a real failure.
            result = runner.run_scenario(scenario, SHIM_DIR, DEVIATIONS)
            OBSERVED[scenario.name] = set(result.differing)
            results.append(result)

        if family.active:
            for result in results:
                with self.subTest(scenario=result.scenario.name):
                    self.assertEqual(
                        result.unforgiven, (),
                        f"{family.name} claims parity but {result.scenario.name} "
                        f"differs on {', '.join(result.unforgiven)}:\n{result.report()}")
            return

        mismatched = [result for result in results if result.differing]
        counts: dict[str, int] = {}
        for result in results:
            for surface in result.differing:
                counts[surface] = counts.get(surface, 0) + 1
        detail = ", ".join(f"{surface} x{count}" for surface, count in
                           sorted(counts.items()))
        self.skipTest(
            f"parity not yet claimed for {family.name} -- "
            f"{len(mismatched)}/{len(results)} cases differ"
            + (f" ({detail})" if detail else ""))

    def check_family_golden(self, family) -> None:
        """Compare each scenario's live Rust output to its checked-in golden.

        No live Python anywhere in this path -- that is the point, it is the
        Phase 4 state -- and no `active` skip either: a golden is not a claim
        about how much of the port has caught up to Python, it is a snapshot of
        what the Rust binary already does, so every scenario in every family is
        checked the same way regardless of the family's `active` flag.

        A missing golden fails rather than skips. scenarios.py growing a case
        that capture_goldens.py has not been run against since is exactly the
        gap this check exists to close before it reaches Phase 4 as a silent
        hole: no Python to fall back on then, and no golden either.
        """
        for scenario in family.scenarios:
            with self.subTest(scenario=scenario.name):
                path = capture_goldens.golden_path(family.name, scenario.name)
                self.assertTrue(
                    path.is_file(),
                    f"no golden for {family.name}/{scenario.name} ({path}) -- "
                    "run `python3 tests/differential/capture_goldens.py`")
                golden = json.loads(path.read_text(encoding="utf-8"))
                capture = capture_goldens.capture_scenario(scenario, SHIM_DIR)
                actual = capture_goldens.capture_dict(capture)
                self.assertEqual(
                    golden, actual,
                    f"{family.name}/{scenario.name} differs from its golden "
                    f"({path}) -- either the port regressed or the golden is "
                    "stale and needs recapturing")

    def test_deviations_are_all_load_bearing(self) -> None:
        """A deviation that stopped applying is a lie; fail like an XPASS does.

        Named to sort after the family methods -- unittest runs a class's tests
        in alphabetical order, and this one is only meaningful once every family
        has recorded what it observed.

        Skipped in golden mode: deviations.toml is a live-Python-vs-Rust
        artifact, DEVIATIONS is loaded empty there (see setUpModule) and
        OBSERVED is never populated by check_family_golden, so there is nothing
        here to check staleness against.
        """
        if GOLDEN:
            self.skipTest("deviations.toml does not apply in golden mode")
        stale = runner.stale_deviations(DEVIATIONS, OBSERVED)
        if not stale:
            return
        lines = "\n".join(
            f"  {item.scenario} / {item.surface}: {item.reason}" for item in stale)
        self.fail(
            "stale deviation -- these entries in deviations.toml document a "
            "difference that did not happen in this run. Either the port caught "
            "up and the entry should be deleted, or the scenario stopped "
            "exercising the thing and the scenario should be fixed:\n" + lines)


def _attach_family_tests() -> None:
    """One test method per family, generated so scenarios.py stays the only list."""
    for family in FAMILIES:
        def method(self, family=family):
            self.check_family(family)
        # a_family_ prefix: alphabetically ahead of
        # test_deviations_are_all_load_bearing, which has to run last.
        method.__name__ = f"test_a_family_{family.name.replace('-', '_')}"
        method.__doc__ = f"{family.name}: {family.note}"
        setattr(RustPortParity, method.__name__, method)


_attach_family_tests()


class HarnessSelfCheck(unittest.TestCase):
    """The harness's own invariants, checked before its verdicts mean anything."""

    def test_both_backends_exist(self) -> None:
        self.assertTrue(runner.RUST_BINARY.is_file(),
                        f"cargo build produced no binary at {runner.RUST_BINARY}")
        if GOLDEN:
            # The Phase 4 state this mode exists for has no Python backend at
            # all -- that absence is one of the two ways GOLDEN itself turns
            # on, so asserting the file's presence here would make this test
            # contradict the mode it is running in.
            self.skipTest("golden mode does not require the Python backend")
        self.assertTrue(runner.BACKEND_PATH.is_file(),
                        f"the Python backend is missing: {runner.BACKEND_PATH}")

    def test_defaults_are_linked_not_copied(self) -> None:
        """The scratch HOME must reproduce the stow shape, or write_marker's
        protection is never exercised and a port that truncates through the
        symlink looks correct here while editing the checkout in the field."""
        scenario = runner.Scenario(name="self-check", argv=("help",))
        with tempfile.TemporaryDirectory(prefix="garage-diff-selfcheck-") as name:
            home = runner.prepare_home(Path(name), scenario)
            link = home / ".config" / "garage" / "preferences.defaults.toml"
            self.assertTrue(link.is_symlink(),
                            "preferences.defaults.toml must be a symlink, not a copy")
            self.assertEqual(Path(link.readlink()), runner.DEFAULTS_SOURCE)

    def test_path_is_shims_only(self) -> None:
        """Anything the backend execs has to land on a shim, so PATH holds
        nothing else. A real binary leaking in would mean the trace recorded a
        call that also changed this machine."""
        scenario = runner.Scenario(name="self-check", argv=("help",))
        with tempfile.TemporaryDirectory(prefix="garage-diff-selfcheck-") as name:
            root = Path(name)
            home = runner.prepare_home(root, scenario)
            environment = runner.case_environment(
                home, Path("/shims"), root / "trace", root / "fixtures.json",
                scenario, "1")
        self.assertEqual(environment["PATH"], "/shims")
        for leaked in ("GARAGE_PREFERENCES", "GARAGE_DISPLAYS",
                       "GARAGE_KEYBINDINGS", "GARAGE_WORKSPACE_BLOCKS"):
            self.assertNotIn(leaked, environment,
                             f"{leaked} would point one run at a different file")

    def test_every_shim_was_materialized(self) -> None:
        self.assertIsNotNone(SHIM_DIR, "setUpModule did not run")
        for name in runner.SHIM_NAMES:
            path = SHIM_DIR / name
            self.assertTrue(path.is_file(), f"shim {name} was not written")
            self.assertTrue(path.stat().st_mode & 0o111, f"shim {name} is not executable")
        if shutil.which("luac"):
            self.assertTrue((SHIM_DIR / "luac").is_file(),
                            "luac exists on this machine but no luac shim was written")
        else:
            self.assertFalse((SHIM_DIR / "luac").exists(),
                             "no luac on this machine, so which('luac') must stay false "
                             "in both backends -- a stub would claim a check ran")
