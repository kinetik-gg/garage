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
import shutil
import sys
import tempfile
import unittest

from . import runner
from .scenarios import FAMILIES


SHIM_DIR: Path | None = None
DEVIATIONS: tuple[runner.Deviation, ...] = ()
OBSERVED: dict[str, set[str]] = {}
_TEMPORARY: tempfile.TemporaryDirectory | None = None


def setUpModule() -> None:
    """Build the Rust binary, materialize the shims, load the deviation list.

    The cargo build happens once for the module, not once per case: it is the
    only slow thing here and it produces the same binary for every scenario.
    """
    global SHIM_DIR, DEVIATIONS, _TEMPORARY
    runner.build_rust()
    # Resolved before anything clamps PATH; see runner.materialize_shims.
    luac = runner.find_luac()
    _TEMPORARY = tempfile.TemporaryDirectory(prefix="garage-diff-shims-")
    SHIM_DIR = Path(_TEMPORARY.name)
    runner.materialize_shims(SHIM_DIR, sys.executable, luac)
    DEVIATIONS = runner.load_deviations()
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

    def test_deviations_are_all_load_bearing(self) -> None:
        """A deviation that stopped applying is a lie; fail like an XPASS does.

        Named to sort after the family methods -- unittest runs a class's tests
        in alphabetical order, and this one is only meaningful once every family
        has recorded what it observed.
        """
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
        self.assertTrue(runner.BACKEND_PATH.is_file(),
                        f"the Python backend is missing: {runner.BACKEND_PATH}")
        self.assertTrue(runner.RUST_BINARY.is_file(),
                        f"cargo build produced no binary at {runner.RUST_BINARY}")

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
