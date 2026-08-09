"""The two commands a broken install is recovered with.

`garage doctor --report` is the bug-reporting pipeline: one JSON blob a user can
paste, carrying the same checks the printed report walks. The property worth
testing is that "the same" is true -- two surfaces over one check list, so a
check cannot reach the transcript and miss the report.

`garage repair` is the way back from a preferences.toml this build cannot parse,
which is the one state layer 2 does not heal on its own. Everything asserted here
is about not losing the user's file on the way: a run without --reset must leave
it byte-for-byte untouched (including not *migrating* it, which the load path
would), and a run with --reset must put the original bytes somewhere that no
later run can overwrite.
"""

from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
import re
import tempfile
import tomllib
import unittest

from harness import BackendTestCase, load_backend


# A file tomllib refuses outright: an unterminated table header, an unterminated
# string, and a bare line. This is the state that leaves the pane read-only --
# every writer loads before it writes -- and so the state repair exists for.
BROKEN = '[appearance\naccent_color = "teal\nnot toml at all\n'

# A status line from the printed report: two spaces, the verdict, the check name
# padded out to the widest one, then the detail. The name is what is being read
# back out, and it may contain a single space ("stow links").
STATUS_LINE = re.compile(r"^  (?:ok|note|FAIL)\s+(.+?)\s{2,}\S")


def transcript_of(function, *arguments) -> tuple[int, str]:
    """Run one of the human commands and capture what it printed."""
    stream = io.StringIO()
    with contextlib.redirect_stdout(stream):
        status = function(*arguments)
    return status, stream.getvalue()


def backups(garage) -> list[Path]:
    return sorted(garage.PREFERENCES_PATH.parent.glob(
        f"{garage.PREFERENCES_PATH.name}.bak-*"))


class DoctorReport(unittest.TestCase):
    """One real report, and one real transcript, for the whole class.

    setUpClass rather than setUp on purpose: the probes shell out to pacman,
    fc-list and systemctl, and running the full check list once per test method
    would pay for that a dozen times over to assert a dozen things about the same
    object. Both surfaces are captured through the command itself -- doctor() --
    so what is asserted is what a user would see and paste, not a helper the CLI
    might not be calling.
    """

    @classmethod
    def setUpClass(cls) -> None:
        directory = tempfile.TemporaryDirectory(prefix="garage-report-")
        cls.addClassCleanup(directory.cleanup)
        cls.root = Path(directory.name)
        cls.garage = load_backend(cls.root)
        # A value this build cannot render, stamped current so the load has no
        # migration to do: it gives preferences_notes something real to carry,
        # which is the one field that quotes a value out of the user's file.
        cls.garage.PREFERENCES_PATH.write_text(
            '[schema]\npreferences_version = 5\n\n'
            '[appearance]\ncorner_radius = "gigantic"\n', encoding="utf-8")
        cls.status, printed = transcript_of(cls.garage.doctor, ["--report"])
        cls.printed = printed
        cls.report = json.loads(printed)
        _, cls.transcript = transcript_of(cls.garage.doctor, [])

    def test_the_command_prints_one_json_object_and_nothing_else(self) -> None:
        # json.loads in setUpClass already proved it parses; this proves there is
        # no banner, no progress line and no trailing note beside it, which is
        # what makes the output safe to pipe.
        self.assertIsInstance(self.report, dict)
        self.assertTrue(self.printed.startswith("{"))
        self.assertEqual(self.printed.strip()[-1], "}")

    def test_the_top_level_keys_are_the_ones_a_bug_report_needs(self) -> None:
        self.assertEqual(sorted(self.report), [
            "checks", "garage_commit", "generated_at", "hyprland_version",
            "packages", "preferences_notes"])
        # ISO 8601 local time with its offset, which is what a person reading
        # their own report expects to see.
        self.assertRegex(self.report["generated_at"],
                         r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[+-]\d{4}$")
        self.assertIsInstance(self.report["preferences_notes"], list)

    def test_every_check_is_a_name_status_detail_and_hint(self) -> None:
        self.assertTrue(self.report["checks"])
        for check in self.report["checks"]:
            self.assertEqual(sorted(check), ["detail", "hint", "name", "status"])
            self.assertIn(check["status"], {"ok", "note", "FAIL"})
            self.assertTrue(check["detail"])
            for field in check.values():
                self.assertIsInstance(field, str)

    def test_the_json_report_names_every_check_the_printed_one_does(self) -> None:
        """The whole reason the probes return their verdict instead of printing it."""
        printed = [match.group(1) for match in
                   (STATUS_LINE.match(line) for line in self.transcript.splitlines())
                   if match]
        self.assertEqual(printed, [check["name"] for check in self.report["checks"]])
        # And the check list is not empty by accident of the regex.
        self.assertIn("preferences", printed)

    def test_the_exit_status_still_answers_the_health_question(self) -> None:
        failed = any(check["status"] == "FAIL" for check in self.report["checks"])
        self.assertEqual(self.status, 1 if failed else 0)

    def test_every_key_package_is_reported_with_a_version_or_null(self) -> None:
        packages = self.report["packages"]
        self.assertEqual(sorted(packages), sorted(self.garage.DOCTOR_PACKAGES))
        for name, version in packages.items():
            self.assertTrue(version is None or isinstance(version, str), name)

    def test_a_coerced_value_reaches_the_notes(self) -> None:
        notes = self.report["preferences_notes"]
        self.assertTrue(any("corner_radius" in note for note in notes), notes)

    def test_no_preference_could_ever_hold_a_credential(self) -> None:
        """The report quotes preference values, so the schema is the whole exposure.

        preferences_notes carries the offending value verbatim -- see
        validate_preferences() -- so "is this blob safe to paste in public" is
        exactly the question "can any preference hold a secret". Today none can:
        Garage authenticates to nothing, and every entry is a colour, a timeout,
        an enum choice, a number, a path, a locale or a command name. This test is
        the tripwire on adding one that could.
        """
        forbidden = re.compile(
            r"password|passwd|secret|token|credential|api_key|apikey|"
            r"private_key|auth|session_key", re.I)
        offenders = [dotted for dotted in self.garage.PREFERENCE_SCHEMA
                     if forbidden.search(dotted)]
        self.assertEqual(offenders, [], "a preference that could hold a credential "
                         "would be quoted verbatim in doctor --report's notes")

    def test_a_missing_git_checkout_is_tolerated_rather_than_fatal(self) -> None:
        # A tarball, a git that is not installed, a repository with no commit.
        # None of them is a reason to refuse a bug report.
        self.assertEqual(self.garage.checkout_commit(self.root), "")

    def test_an_unknown_argument_is_refused(self) -> None:
        with self.assertRaises(self.garage.SettingsError):
            self.garage.doctor(["--repot"])


class Repair(BackendTestCase):
    """`garage repair`, against a preferences.toml that will not parse."""

    def break_preferences(self) -> bytes:
        self.garage.PREFERENCES_PATH.write_text(BROKEN, encoding="utf-8")
        return self.garage.PREFERENCES_PATH.read_bytes()

    def fix_the_backup_stamp(self) -> None:
        """Pin the backup timestamp so the collision path is not a race to test.

        The name carries a whole-second stamp, so two repairs in the same second
        collide -- which is the case worth asserting and the one a test cannot
        schedule. Pinning it makes every run of the test the colliding run.
        """
        original = self.garage.BACKUP_STAMP
        self.garage.BACKUP_STAMP = "fixed"
        self.addCleanup(setattr, self.garage, "BACKUP_STAMP", original)

    def stored(self) -> dict:
        with self.garage.PREFERENCES_PATH.open("rb") as handle:
            return tomllib.load(handle)

    def test_a_run_without_reset_changes_nothing(self) -> None:
        before = self.break_preferences()
        stat = self.garage.PREFERENCES_PATH.stat()
        status, printed = transcript_of(self.garage.repair, [])
        self.assertEqual(status, 0)
        self.assertEqual(self.garage.PREFERENCES_PATH.read_bytes(), before)
        self.assertEqual(self.garage.PREFERENCES_PATH.stat().st_mtime_ns, stat.st_mtime_ns)
        self.assertEqual(backups(self.garage), [])
        # And it says both halves out loud: what it saw, and that it did nothing.
        self.assertIn("does NOT parse", printed)
        self.assertIn("Nothing has been changed.", printed)

    def test_a_run_without_reset_does_not_migrate_the_file_either(self) -> None:
        """The preview reads with tomllib, not through the load path.

        load_preferences() migrates, and v5's migration rewrites the file. A
        command whose first mode is "tell me what you see" must not be the thing
        that changed it -- so a v4 full document has to come out of a preview run
        exactly as it went in.
        """
        document = self.garage.deep_merge(self.garage.FALLBACK_DEFAULTS,
                                         {"appearance": {"accent_color": "teal"}})
        document["schema"] = {"preferences_version": 4}
        self.garage.PREFERENCES_PATH.write_text(self.garage.dump_toml(document),
                                                encoding="utf-8")
        before = self.garage.PREFERENCES_PATH.read_bytes()
        transcript_of(self.garage.repair, [])
        self.assertEqual(self.garage.PREFERENCES_PATH.read_bytes(), before)

    def test_reset_keeps_the_original_bytes_in_a_backup(self) -> None:
        before = self.break_preferences()
        status, printed = transcript_of(self.garage.repair, ["--reset"])
        self.assertEqual(status, 0)
        kept = backups(self.garage)
        self.assertEqual(len(kept), 1)
        self.assertEqual(kept[0].read_bytes(), before)
        self.assertRegex(kept[0].name,
                         rf"^{self.garage.PREFERENCES_PATH.name}\.bak-\d{{8}}-\d{{6}}$")
        # Where it went has to be in the output, or the backup might as well not
        # exist: nothing else ever names it again.
        self.assertIn(kept[0].name, printed)

    def test_reset_writes_a_stamp_only_file_that_loads_clean(self) -> None:
        self.break_preferences()
        _, printed = transcript_of(self.garage.repair, ["--reset"])
        stored = self.stored()
        self.assertEqual(stored["schema"]["preferences_version"],
                         self.garage.PREFERENCES_VERSION)
        # Stamp only: under the deltas model an empty file *is* factory state, and
        # writing the shipped values out would pin today's defaults forever.
        self.assertEqual(self.garage.preference_sections(stored), {})
        self.assertIn("loads with every value in range", printed)

    def test_the_repaired_file_is_the_shipped_configuration(self) -> None:
        self.break_preferences()
        transcript_of(self.garage.repair, ["--reset"])
        effective = self.garage.load_preferences()
        expected = self.garage.validate_preferences(
            self.garage.deep_merge(self.garage.shipped_defaults(), {}))
        self.assertEqual(self.garage.preference_sections(effective),
                         self.garage.preference_sections(expected))
        # The load left the file alone: it was already current.
        self.assertEqual(self.garage.preference_sections(self.stored()), {})

    def test_reset_never_overwrites_an_existing_backup(self) -> None:
        self.fix_the_backup_stamp()
        before = self.break_preferences()
        transcript_of(self.garage.repair, ["--reset"])
        fresh = self.garage.PREFERENCES_PATH.read_bytes()
        transcript_of(self.garage.repair, ["--reset"])
        kept = backups(self.garage)
        self.assertEqual([path.name for path in kept],
                         [f"{self.garage.PREFERENCES_PATH.name}.bak-fixed",
                          f"{self.garage.PREFERENCES_PATH.name}.bak-fixed-2"])
        # The first run's backup -- the only copy of what the user actually wrote
        # -- survives the second run untouched.
        self.assertEqual(kept[0].read_bytes(), before)
        self.assertEqual(kept[1].read_bytes(), fresh)

    def test_reset_on_a_fresh_install_writes_a_file_and_backs_nothing_up(self) -> None:
        self.assertFalse(self.garage.PREFERENCES_PATH.exists())
        status, printed = transcript_of(self.garage.repair, ["--reset"])
        self.assertEqual(status, 0)
        self.assertEqual(backups(self.garage), [])
        self.assertEqual(self.stored()["schema"]["preferences_version"],
                         self.garage.PREFERENCES_VERSION)
        self.assertIn("none needed", printed)

    def test_only_preferences_toml_is_touched(self) -> None:
        """The other three user files are records with their own recovery."""
        self.break_preferences()
        for path, text in ((self.garage.DISPLAYS_PATH, 'primary = "DP-1"\n'),
                           (self.garage.KEYBINDINGS_PATH, "[overrides]\n"),
                           (self.garage.WORKSPACE_BLOCKS_PATH, "[block]\n")):
            path.write_text(text, encoding="utf-8")
        before = {path: path.read_bytes() for path in
                  (self.garage.DISPLAYS_PATH, self.garage.KEYBINDINGS_PATH,
                   self.garage.WORKSPACE_BLOCKS_PATH)}
        _, printed = transcript_of(self.garage.repair, ["--reset"])
        for path, data in before.items():
            self.assertEqual(path.read_bytes(), data, path.name)
        # And it says so, so nobody runs it expecting their shortcuts back.
        self.assertIn("preferences.toml only", printed)

    def test_an_unknown_argument_is_refused(self) -> None:
        self.break_preferences()
        with self.assertRaises(self.garage.SettingsError):
            self.garage.repair(["--rest"])
        self.assertEqual(backups(self.garage), [])


if __name__ == "__main__":
    unittest.main()
