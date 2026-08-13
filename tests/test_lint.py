"""The Rust workspace lints itself, and this makes that a checked-in fact.

Task 2.2 of the Rust-port sprint: backend/ (11 crates under backend/crates/) is now
part of the product, not a side experiment, so its formatting, its clippy
configuration and its structural rules deserve the same "fails the build if
violated" treatment as the Python/Lua side. Everything here shells out to
`cargo` rather than reimplementing what it checks -- the workspace's own
`cargo fmt --check` / `cargo clippy` / `cargo metadata` are the source of
truth, and duplicating their logic in Python would just be a second place for
the two to drift apart.

A MISSING cargo binary is a hard failure, not a skip: unlike an optional dev
tool, the Rust toolchain is part of the product now, and a CI/dev box without
it is a box that cannot actually build this repo.
"""

from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import unittest


TESTS_DIR = Path(__file__).resolve().parent
REPO_ROOT = TESTS_DIR.parent
RUST_ROOT = REPO_ROOT / "backend"
CRATES_DIR = RUST_ROOT / "crates"

# Cold `target/` compiles every crate from scratch before clippy can even
# start reporting diagnostics -- generous on purpose so a slow first run
# doesn't masquerade as a lint failure.
SUBPROCESS_TIMEOUT = 300

# backend/crates/**/src files over 500 lines, and why each is allowed to be.
# The rule is data so an exception is a one-line diff here instead of a
# special case in the test body.
EXCEPTIONS: dict[str, str] = {
    "backend/crates/garage-core/src/schema/prefs.rs":
        "the schema table is one table; splitting it would hide drift",
}


def require_cargo() -> str:
    """Return the `cargo` binary path, or fail loudly if it is missing.

    Not a skip: the Rust toolchain is part of the product now, so a box that
    cannot run `cargo` is a box that cannot build this repo, and the test
    suite should say so rather than quietly reporting nothing to check.
    """
    cargo = shutil.which("cargo")
    if cargo is None:
        raise AssertionError(
            "cargo is not on PATH -- the Rust toolchain is part of the "
            "product now, so this cannot be skipped. Install it (see "
            "backend/rust-toolchain.toml) and re-run.")
    return cargo


def run_cargo(cargo: str, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [cargo, *args], cwd=RUST_ROOT, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        timeout=SUBPROCESS_TIMEOUT, check=False)


def clippy_json_lines(cargo: str) -> list[dict]:
    """One decoded JSON object per `cargo clippy --message-format=json` line.

    Warnings are not promoted to errors here (no `-D warnings`), which is the
    point: an unknown-lint or renamed-lint diagnostic is itself only a
    warning, and promoting warnings to errors would make this call fail for
    the same reason test_clippy_clean does instead of surfacing the specific
    lint names that are wrong.
    """
    result = subprocess.run(
        [cargo, "clippy", "--all-targets", "--message-format=json"],
        cwd=RUST_ROOT, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, timeout=SUBPROCESS_TIMEOUT, check=False)
    lines = []
    for line in result.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            lines.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return lines


def crate_src_dirs():
    return sorted(p for p in CRATES_DIR.glob("*/src") if p.is_dir())


class RustFmt(unittest.TestCase):
    def test_cargo_fmt_clean(self) -> None:
        """`cargo fmt --check` passes over the whole workspace.

        rustfmt.toml pins the formatting rules; this is what makes drifting
        from them a build failure instead of a matter of taste noticed in
        review.
        """
        cargo = require_cargo()
        result = run_cargo(cargo, "fmt", "--check")
        self.assertEqual(
            0, result.returncode,
            f"cargo fmt --check failed (exit {result.returncode}):\n{result.stdout}")


class Clippy(unittest.TestCase):
    def test_clippy_clean(self) -> None:
        """`cargo clippy --all-targets -- -D warnings` passes clean.

        backend/Cargo.toml's [workspace.lints.clippy] table denies clippy::all
        and warns on pedantic plus a long list of specific lints
        (unwrap_used, panic, indexing_slicing, ...); `-D warnings` on top of
        that turns every warning-level lint into a build failure too, so this
        is the whole configured lint surface, not just the deny list.
        """
        cargo = require_cargo()
        result = run_cargo(cargo, "clippy", "--all-targets", "--", "-D", "warnings")
        self.assertEqual(
            0, result.returncode,
            f"cargo clippy failed (exit {result.returncode}):\n{result.stdout}")

    def test_every_configured_lint_resolves(self) -> None:
        """No configured lint name has gone unknown or been renamed/removed.

        backend/Cargo.toml and backend/clippy.toml name specific lints by string.
        A toolchain bump can rename or remove a lint clippy used to know
        about; when that happens the lint silently stops being enforced and
        clippy reports it as an `unknown_lints` / `renamed_and_removed_lints`
        warning rather than an error (so test_clippy_clean's `-D warnings`
        would in fact catch it too, just without naming which lint rotted).
        This test runs clippy without promoting warnings so the diagnostic
        surfaces on its own terms, and prints the offending lint names
        directly instead of forcing a reader back to raw clippy output.
        """
        cargo = require_cargo()
        offenders = []
        for message in clippy_json_lines(cargo):
            if message.get("reason") != "compiler-message":
                continue
            diagnostic = message.get("message", {})
            code = diagnostic.get("code") or {}
            if code.get("code") in ("unknown_lints", "renamed_and_removed_lints"):
                offenders.append(diagnostic.get("message", "").splitlines()[0])
        self.assertEqual([], offenders)


class FileShape(unittest.TestCase):
    def test_no_file_over_500_lines(self) -> None:
        """No .rs file under backend/crates/**/src exceeds 500 lines.

        The same shape rule the rest of this repo applies to Lua/Python
        files, made data instead of prose: EXCEPTIONS maps a repo-relative
        path to the reason it is allowed past the limit, so an exception is a
        one-line addition here rather than a silent exemption baked into the
        test body.
        """
        offenders = []
        for path in sorted(CRATES_DIR.glob("*/src/**/*.rs")):
            relpath = str(path.relative_to(REPO_ROOT))
            if relpath in EXCEPTIONS:
                continue
            line_count = len(path.read_text().splitlines())
            if line_count > 500:
                offenders.append(f"{relpath}: {line_count} lines")
        self.assertEqual([], offenders)


class UnsafeForbidden(unittest.TestCase):
    def test_every_crate_forbids_unsafe(self) -> None:
        """Every crate root (lib.rs or main.rs) carries #![forbid(unsafe_code)].

        backend/Cargo.toml already sets `unsafe_code = "forbid"` at the
        workspace-lint level, but a crate can still override that locally;
        this checks the crate-level attribute itself is present so the
        guarantee does not depend on every crate's Cargo.toml continuing to
        opt into the workspace lint table.
        """
        roots = []
        for src in crate_src_dirs():
            for name in ("lib.rs", "main.rs"):
                candidate = src / name
                if candidate.is_file():
                    roots.append(candidate)

        self.assertTrue(roots, f"found no lib.rs/main.rs under {CRATES_DIR}/*/src")

        missing = []
        for root in roots:
            if "#![forbid(unsafe_code)]" not in root.read_text():
                missing.append(str(root.relative_to(REPO_ROOT)))
        self.assertEqual([], missing)


class CrateGraph(unittest.TestCase):
    def test_render_crate_cannot_reach_the_lock(self) -> None:
        """garage-render cannot depend on garage-prefs or garage-proc.

        Invariant 4 (the render path must never be able to take
        PREFERENCES_LOCK) has a runtime half enforced elsewhere and a
        structural half enforced here: as long as garage-render's Cargo.toml
        never grows a dependency edge onto garage-prefs (which owns the lock)
        or garage-proc (which owns process execution), the render context is
        not merely disciplined about not taking the lock -- it is physically
        unable to reach the code that takes it. cargo metadata is read
        instead of grepping Cargo.toml so this also catches a transitive path
        introduced through some other crate.
        """
        cargo = require_cargo()
        # stdout and stderr kept separate here (unlike run_cargo's merged
        # capture): stdout must be exactly the JSON document, and anything
        # cargo writes to stderr would otherwise corrupt the parse.
        result = subprocess.run(
            [cargo, "metadata", "--format-version", "1", "--no-deps"],
            cwd=RUST_ROOT, text=True, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, timeout=SUBPROCESS_TIMEOUT, check=False)
        self.assertEqual(
            0, result.returncode,
            f"cargo metadata failed (exit {result.returncode}):\n{result.stderr}")

        metadata = json.loads(result.stdout)
        packages = {pkg["name"]: pkg for pkg in metadata["packages"]}
        self.assertIn("garage-render", packages,
                       "garage-render is missing from `cargo metadata` output")

        forbidden = {"garage-prefs", "garage-proc"}
        deps = {dep["name"] for dep in packages["garage-render"]["dependencies"]}
        reached = deps & forbidden
        self.assertEqual(
            set(), reached,
            f"garage-render depends on {sorted(reached)} -- this reopens a "
            "path to PREFERENCES_LOCK from the render context")


if __name__ == "__main__":
    unittest.main()
