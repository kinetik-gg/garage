"""Background filename indexing stays bounded, complete, and queryable."""

from __future__ import annotations

import fcntl
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

try:
    from harness import BackendTestCase
except ModuleNotFoundError:  # `python -m unittest tests.test_file_index`
    from tests.harness import BackendTestCase


REPO_ROOT = Path(__file__).resolve().parent.parent
INDEXER = REPO_ROOT / "desktop" / ".local" / "bin" / "garage-file-index"


def load_indexer(root: Path):
    environment = {
        "HOME": str(root),
        "XDG_CONFIG_HOME": str(root / ".config"),
        "XDG_CACHE_HOME": str(root / ".cache"),
        "GARAGE_FILE_INDEX_DB": str(root / ".cache" / "garage" / "files.sqlite3"),
    }
    with mock.patch.dict(os.environ, environment, clear=False):
        loader = importlib.machinery.SourceFileLoader(
            f"garage_file_index_{id(root)}", str(INDEXER))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        module = importlib.util.module_from_spec(spec)
        loader.exec_module(module)
        return module


class FileIndex(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory(prefix="garage-file-index-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.indexer = load_indexer(self.root)

    def config(self, roots: list[Path], depth: int = 8) -> dict:
        return {"enabled": True, "frequency_minutes": 5,
                "max_depth": depth, "directories": roots}

    def test_refresh_replaces_the_index_and_searches_names_and_paths(self) -> None:
        documents = self.root / "Documents"
        nested = documents / "Clients" / "Kinetik"
        nested.mkdir(parents=True)
        (documents / "Budget 2026.ods").write_text("numbers", encoding="utf-8")
        (nested / "Launch Notes.md").write_text("notes", encoding="utf-8")

        first = self.indexer.refresh_index(self.config([documents]))
        self.assertEqual(4, first["count"])
        self.assertEqual("Budget 2026.ods",
                         self.indexer.search_index("budget", 8)[0]["title"])
        self.assertEqual("Launch Notes.md",
                         self.indexer.search_index("kinetik launch", 8)[0]["title"])

        # A reader must keep seeing the last committed snapshot while the next
        # scan has cleared and is rebuilding its private transaction.
        writer = self.indexer.connect()
        writer.execute("BEGIN IMMEDIATE")
        writer.execute("DELETE FROM files")
        self.assertEqual("Budget 2026.ods",
                         self.indexer.search_index("budget", 8)[0]["title"])
        writer.rollback()
        writer.close()

        (documents / "Budget 2026.ods").unlink()
        self.indexer.refresh_index(self.config([documents]))
        self.assertEqual([], self.indexer.search_index("budget", 8))

    def test_depth_hidden_directories_and_symlinks_are_bounded(self) -> None:
        root = self.root / "Projects"
        (root / "one" / "two").mkdir(parents=True)
        (root / "one" / "visible.txt").write_text("", encoding="utf-8")
        (root / "one" / "two" / "too-deep.txt").write_text("", encoding="utf-8")
        (root / ".git").mkdir()
        (root / ".git" / "secret.txt").write_text("", encoding="utf-8")
        (root / "node_modules").mkdir()
        (root / "node_modules" / "package.js").write_text("", encoding="utf-8")
        (root / "linked").symlink_to(root / "one", target_is_directory=True)

        self.indexer.refresh_index(self.config([root], depth=2))

        self.assertEqual("visible.txt", self.indexer.search_index("visible", 8)[0]["title"])
        for hidden in ("too-deep", "secret", "package", "linked"):
            with self.subTest(hidden=hidden):
                self.assertEqual([], self.indexer.search_index(hidden, 8))

    def test_configured_roots_are_deduplicated_and_confined_to_home(self) -> None:
        inside = self.root / "Documents"
        roots = self.indexer.configured_roots(
            f"{inside}\n{inside}\n/etc\n../outside\n")
        self.assertEqual([inside], roots)

        roots = self.indexer.configured_roots(
            f"{inside}\n{self.root}\n{inside / 'Projects'}\n")
        self.assertEqual([self.root], roots)

    def test_status_reports_activity_and_the_last_complete_snapshot(self) -> None:
        documents = self.root / "Documents"
        documents.mkdir()
        (documents / "notes.txt").write_text("", encoding="utf-8")

        before = self.indexer.status()
        self.assertEqual("not_indexed", before["activity"])

        self.indexer.refresh_index(self.config([documents]))
        after = self.indexer.status()
        self.assertEqual("idle", after["activity"])
        self.assertEqual(1, after["count"])
        self.assertGreater(after["last_scan_epoch"], 0)

        self.indexer.INDEX_LOCK_PATH.parent.mkdir(parents=True, exist_ok=True)
        with self.indexer.INDEX_LOCK_PATH.open("a+", encoding="utf-8") as lock:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
            self.assertEqual("indexing", self.indexer.status()["activity"])


class FileIndexLifecycle(unittest.TestCase):
    def test_bootstrap_enables_a_low_priority_background_service(self) -> None:
        service = (REPO_ROOT / "desktop" / ".config" / "systemd" / "user"
                   / "garage-file-index.service").read_text(encoding="utf-8")
        # bootstrap.sh enables whatever system/manifest/units.list names, so
        # that file is where the unit has to appear for the installer to reach
        # it. `running`: it is a daemon, not a Type=oneshot task.
        units = (REPO_ROOT / "system" / "manifest"
                 / "units.list").read_text(encoding="utf-8")
        self.assertRegex(units, r"(?m)^garage-file-index\.service\s+running\b")
        self.assertIn("Nice=10", service)
        self.assertIn("IOSchedulingClass=idle", service)
        self.assertIn("CPUSchedulingPolicy=batch", service)


class FileIndexPreferenceRouting(BackendTestCase):
    def test_enabled_indexing_restarts_the_service_for_an_immediate_scan(self) -> None:
        config = {"indexing": {"enabled": True}}
        with mock.patch.object(self.garage, "run_or_raise") as run:
            self.garage.apply_file_index(config)
        commands = [call.args[1] for call in run.call_args_list]
        self.assertEqual([
            ["systemctl", "--user", "enable", "garage-file-index.service"],
            ["systemctl", "--user", "restart", "garage-file-index.service"],
        ], commands)

    def test_disabled_indexing_stops_and_disables_the_service(self) -> None:
        config = {"indexing": {"enabled": False}}
        with mock.patch.object(self.garage, "run_or_raise") as run:
            self.garage.apply_file_index(config)
        self.assertEqual(
            ["systemctl", "--user", "disable", "--now",
             "garage-file-index.service"],
            run.call_args.args[1])


if __name__ == "__main__":
    unittest.main()
