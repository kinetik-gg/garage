"""`garage migrate` runs the shipped registry against an isolated home."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from differential import runner


class MigrateCli(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.binary = runner.build_rust()

    def run_migrate(self, *arguments: str) -> tuple[subprocess.CompletedProcess[str], Path]:
        temporary = tempfile.TemporaryDirectory(prefix="garage-migrate-cli-")
        self.addCleanup(temporary.cleanup)
        home = Path(temporary.name)
        environment = {
            "HOME": str(home),
            "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_STATE_HOME": str(home / ".local" / "state"),
            "PATH": os.environ.get("PATH", ""),
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
        }
        completed = subprocess.run(
            [str(self.binary), "migrate", *arguments],
            cwd=home,
            env=environment,
            capture_output=True,
            text=True,
            check=False,
        )
        return completed, home / ".local" / "state" / "garage" / "migrations.json"

    def test_empty_registry_has_nothing_to_apply_and_stamps_nothing(self) -> None:
        completed, stamp = self.run_migrate()
        self.assertEqual(completed.returncode, 0)
        self.assertEqual(completed.stdout, "Garage migrate\n\nNothing to apply.\n")
        self.assertEqual(completed.stderr, "")
        self.assertFalse(stamp.exists())

    def test_empty_registry_dry_run_is_read_only(self) -> None:
        completed, stamp = self.run_migrate("--dry-run")
        self.assertEqual(completed.returncode, 0)
        self.assertEqual(
            completed.stdout,
            "Garage migrate (dry run: nothing will be changed)\n\nNothing to apply.\n",
        )
        self.assertEqual(completed.stderr, "")
        self.assertFalse(stamp.exists())

    def test_unknown_argument_uses_the_plain_command_catch_tier(self) -> None:
        completed, stamp = self.run_migrate("--dryrun")
        self.assertEqual(completed.returncode, 1)
        self.assertEqual(completed.stdout, "")
        self.assertEqual(
            completed.stderr,
            "garage migrate: Usage: garage migrate [--dry-run]  "
            "(unexpected argument: --dryrun)\n",
        )
        self.assertFalse(stamp.exists())


if __name__ == "__main__":
    unittest.main()
