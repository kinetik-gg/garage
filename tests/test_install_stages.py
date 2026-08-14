"""The documented install-stage table stays tied to the executable stages.

bootstrap.sh deliberately keeps its phase runner small, while INSTALL.md gives
each phase its human-readable contract. The table's File column is the shared
inventory: a stage added on only one side is either undocumented code or prose
that names code the runner cannot execute.

The checks are derived from the checkout rather than a fixed list of today's
14 stages. Adding a stage therefore requires a contiguous number, a table row,
valid shell syntax, and a runner whose minimum-count guard moves with it.
"""

from __future__ import annotations

import re
import subprocess
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
INSTALL_DIR = REPO_ROOT / "install"
INSTALL_DOC = REPO_ROOT / "docs" / "INSTALL.md"
STAGE_PATTERN = "[0-9][0-9]-*.sh"
TABLE_HEADING = "## What bootstrap does"


def stage_files() -> list[Path]:
    """The numbered stage files bootstrap is expected to source, in order."""
    return sorted(INSTALL_DIR.glob(STAGE_PATTERN))


def install_table_rows() -> list[tuple[int, str]]:
    """Return (step number, File cell) from the What bootstrap does table."""
    document = INSTALL_DOC.read_text()
    _, separator, section = document.partition(TABLE_HEADING)
    if not separator:
        raise AssertionError(f"{INSTALL_DOC} has no {TABLE_HEADING!r} section")

    table_lines = []
    for line in section.splitlines():
        if line.startswith("|"):
            table_lines.append(line)
        elif table_lines:
            break
    if len(table_lines) < 3:
        raise AssertionError(f"{TABLE_HEADING!r} has no Markdown table")

    def cells(line: str) -> list[str]:
        return [cell.strip() for cell in line.strip().strip("|").split("|")]

    headings = cells(table_lines[0])
    try:
        number_column = headings.index("#")
        file_column = headings.index("File")
    except ValueError as error:
        raise AssertionError(
            f"{TABLE_HEADING!r} table must have # and File columns"
        ) from error

    rows = []
    for line in table_lines[2:]:
        row = cells(line)
        file_cell = re.fullmatch(r"`([^`]+)`", row[file_column])
        if file_cell is None:
            raise AssertionError(f"File cell is not one backticked path: {row[file_column]!r}")
        rows.append((int(row[number_column]), file_cell.group(1)))
    return rows


class InstallStages(unittest.TestCase):
    def test_table_names_every_stage_file(self) -> None:
        """The File column and install's numbered scripts name the same set."""
        on_disk = {
            path.relative_to(REPO_ROOT).as_posix()
            for path in stage_files()
        }
        documented = {file_name for _, file_name in install_table_rows()}
        self.assertIn("bootstrap.sh", documented)
        documented.remove("bootstrap.sh")
        self.assertSetEqual(on_disk, documented)

    def test_stage_numbers_are_contiguous(self) -> None:
        """Stage numbers start at 01 and have neither gaps nor duplicates."""
        numbers = [int(path.name[:2]) for path in stage_files()]
        self.assertEqual(list(range(1, len(numbers) + 1)), numbers)

    def test_shell_syntax(self) -> None:
        """Every installer shell file parses as Bash.

        This is the suite's first shell-syntax enforcement. It covers both
        entry points, the shared library, every numbered stage, and any future
        shell file added under install/.
        """
        shell_files = [
            REPO_ROOT / "bootstrap.sh",
            REPO_ROOT / "install.sh",
            INSTALL_DIR / "lib.sh",
            *sorted(INSTALL_DIR.glob("*.sh")),
        ]
        failures = []
        for path in dict.fromkeys(shell_files):
            result = subprocess.run(
                ["bash", "-n", str(path)],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
            )
            if result.returncode:
                relative = path.relative_to(REPO_ROOT)
                failures.append(f"{relative}: {result.stderr.strip()}")
        self.assertEqual([], failures)

    def test_runner_sources_the_stages(self) -> None:
        """The bootstrap runner keeps its stage loop and minimum-count guard."""
        bootstrap = (REPO_ROOT / "bootstrap.sh").read_text()
        self.assertRegex(
            bootstrap,
            r'stages=\("\$repo_dir"/install/\[0-9\]\[0-9\]-\*\.sh\)',
        )
        self.assertRegex(
            bootstrap,
            rf'if \(\(\$\{{#stages\[@\]\}} < {len(stage_files())}\)\); then',
        )
        self.assertRegex(
            bootstrap,
            r'for stage in "\$\{stages\[@\]\}"; do\s+'
            r'(?:#[^\n]*\s+)*source "\$stage"\s+done',
        )


if __name__ == "__main__":
    unittest.main()
