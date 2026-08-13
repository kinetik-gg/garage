"""The manifest files and the backend's DOCTOR_* tuples still say the same thing.

TEMPORARY. This whole module is a drift guard for the parity window opened by
task 2.8, and it is meant to be deleted in Phase 4.

Task 2.8 moved the package set, the per-user unit list and the font families out
of `bootstrap.sh` arrays and into `system/manifest/*.list`, so that bootstrap,
the Rust port and the Python backend can read one copy instead of three. The
bootstrap arrays are gone -- there is nothing left to diff them against, and a
test asserting `packages.list == the packages=() array` would now be asserting
against a loop that reads packages.list, which proves nothing.

What is *not* gone is the backend's DOCTOR_PACKAGES / DOCTOR_UNITS /
DOCTOR_FONTS. Those stay untouched on purpose: parity first, and a doctor that
changes shape in the same commit as the data files would leave nobody able to
say which side broke. So for the length of the window there are two copies of
that subset, and this module is what makes them impossible to edit apart. Every
assertion runs in both directions -- a name in the doctor that is missing from
the file, and a flagged line in the file that the doctor does not name, are both
failures.

Phase 4 retires the DOCTOR_* constants in favour of reading the manifests
directly. When that lands, the thing this module compares no longer exists and
the module goes with it. Do not extend it into a general manifest test: the real
parser is exercised by backend/crates/garage-core/src/manifest.rs's own tests, and
the format itself is documented in the file headers.

The parsing below is the same three lines bootstrap.sh runs -- strip a trailing
`#` comment, split into fields, skip what is left of a blank line -- transcribed
rather than shared, because a shared helper that agreed with a broken bootstrap
would hide exactly the drift this is here to catch.
"""

from __future__ import annotations

import unittest

from harness import REPO_ROOT, BackendTestCase


MANIFEST_DIR = REPO_ROOT / "system" / "manifest"


def records(name: str) -> list[list[str]]:
    """Every non-comment line of a manifest, split into fields.

    bootstrap.sh:
        manifest_line=${manifest_line%%#*}
        read -r field_one _ <<<"$manifest_line"
        [[ -n $field_one ]] || continue
    """
    rows = []
    for line in (MANIFEST_DIR / name).read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0]
        fields = line.split()
        if fields:
            rows.append(fields)
    return rows


def families(name: str) -> list[str]:
    """Every non-comment line of a manifest, whole.

    fonts.list only. A fontconfig family name contains spaces, so that file
    cannot have a flag column and the record is the entire line.
    """
    lines = []
    for line in (MANIFEST_DIR / name).read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            lines.append(line)
    return lines


class PackageParity(BackendTestCase):
    def test_manifest_parses_to_something(self) -> None:
        """packages.list yields packages, and some of them are critical.

        Guards the parsing above as much as the file: a rule that silently
        matched nothing would make every parity assertion below vacuous.
        """
        rows = records("packages.list")
        self.assertGreater(len(rows), 50, "packages.list parsed to almost nothing")
        flagged = [row for row in rows if row[1:] == ["critical"]]
        self.assertTrue(flagged, "no line in packages.list carries the critical flag")

    def test_only_known_flags(self) -> None:
        """`critical` is the only second field packages.list defines.

        A misspelt flag would otherwise be read as "not critical" by every
        reader and quietly drop a package out of the doctor's list.
        """
        offenders = [" ".join(row) for row in records("packages.list")
                     if len(row) > 2 or row[1:] not in ([], ["critical"])]
        self.assertEqual([], offenders)

    def test_critical_flags_match_doctor_packages(self) -> None:
        """The critical-flagged names are exactly DOCTOR_PACKAGES.

        Both directions: a package the doctor checks but the file does not
        flag would stop being checked the moment the doctor reads the file,
        and a flag the doctor does not name is a claim nothing acts on.
        """
        flagged = {row[0] for row in records("packages.list") if row[1:] == ["critical"]}
        self.assertEqual(set(self.garage.DOCTOR_PACKAGES), flagged)

    def test_doctor_packages_are_installed_packages(self) -> None:
        """Every critical name is also a package bootstrap installs.

        The flag is a column on a package line, so this cannot fail by
        construction today -- it is here for the case where someone "fixes" a
        parity failure by adding a bare name to satisfy the test above.
        """
        names = [row[0] for row in records("packages.list")]
        self.assertEqual(sorted(set(names)), sorted(names), "duplicate package names")
        missing = [name for name in self.garage.DOCTOR_PACKAGES if name not in names]
        self.assertEqual([], missing)


class UnitParity(BackendTestCase):
    KINDS = ("running", "oneshot")

    def test_every_unit_carries_a_known_kind(self) -> None:
        """Each units.list line is `NAME running` or `NAME oneshot`.

        bootstrap.sh refuses to enable a unit whose kind is missing or
        unrecognised, so an unflagged line is a failed install, not a silent
        one -- this catches it here instead.
        """
        offenders = [" ".join(row) for row in records("units.list")
                     if len(row) != 2 or row[1] not in self.KINDS]
        self.assertEqual([], offenders)

    def test_doctor_units_appear_with_the_same_kind(self) -> None:
        """Every DOCTOR_UNITS entry is in units.list with the matching kind.

        DOCTOR_UNITS pairs a name with a bool: True means a live session
        should show it running, False means Type=oneshot and `inactive` is
        healthy. That bool and the file's keyword are the same fact.
        """
        kinds = {row[0]: row[1] for row in records("units.list")}
        mismatched = []
        for name, live in self.garage.DOCTOR_UNITS:
            expected = "running" if live else "oneshot"
            if kinds.get(name) != expected:
                mismatched.append(f"{name}: doctor says {expected}, "
                                  f"units.list says {kinds.get(name)!r}")
        self.assertEqual([], mismatched)

    def test_doctor_checks_a_subset_not_a_superset(self) -> None:
        """The doctor names no unit bootstrap does not enable.

        The other direction is deliberately *not* asserted: units.list is the
        full set bootstrap enables and DOCTOR_UNITS is a subset of it, the
        same way DOCTOR_PACKAGES is a subset of the package set.
        """
        names = {row[0] for row in records("units.list")}
        unknown = [name for name, _ in self.garage.DOCTOR_UNITS if name not in names]
        self.assertEqual([], unknown)


class FontParity(BackendTestCase):
    def test_fonts_list_matches_doctor_fonts(self) -> None:
        """fonts.list is DOCTOR_FONTS, in order and with the spaces intact.

        Order is asserted rather than set equality because there are two of
        them and the file is short enough that a reordering is a real edit
        worth noticing, not incidental churn.
        """
        self.assertEqual(list(self.garage.DOCTOR_FONTS), families("fonts.list"))

    def test_family_names_are_not_package_names(self) -> None:
        """The families are fc-list names, which is why there is no flag column.

        A family name with a space in it cannot survive a last-field-is-a-flag
        layout; this is the check that keeps someone from adding one.
        """
        self.assertTrue(any(" " in family for family in families("fonts.list")))
        self.assertEqual([], [f for f in families("fonts.list") if f != f.strip()])


class ManagedPaths(unittest.TestCase):
    """No DOCTOR_* counterpart, so this is a format check, not a parity one.

    managed-paths.list is new in the same commit and has nothing in the backend
    to drift from yet, but the same three-line parse has to hold for it or the
    Rust reader and bootstrap would disagree about the file that describes
    every other file.
    """

    KINDS = ("stow-tree", "generated", "artifact", "override")

    def test_every_row_is_kind_path_owner(self) -> None:
        offenders = [" ".join(row) for row in records("managed-paths.list")
                     if not 2 <= len(row) <= 3 or row[0] not in self.KINDS]
        self.assertEqual([], offenders)

    def test_the_stow_tree_is_named(self) -> None:
        """`stow-tree desktop/` is the row the other kinds are exceptions to."""
        rows = records("managed-paths.list")
        self.assertIn(["stow-tree", "desktop/"], rows)

    def test_paths_are_relative(self) -> None:
        """No absolute path and no `~/`: every row is resolved against $HOME."""
        offenders = [row[1] for row in records("managed-paths.list")
                     if row[1].startswith(("/", "~"))]
        self.assertEqual([], offenders)


if __name__ == "__main__":
    unittest.main()
