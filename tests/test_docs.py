"""Documentation citations stay pinned to the code they claim to describe.

docs/*.md cites the backend by function name, by constant name, and by
`file.lua:N` source line. Nothing enforces any of that as the code moves
underneath it -- a rename or a deleted constant leaves a citation pointing at
nothing, and a reader only discovers it by following the citation and finding
it wrong. So this flattens every citation out of the docs (the same
extract-both-sides-and-diff discipline test_schema.py uses for the preference
schema) and checks each one against the code it names.

commit 674a56e rewrote every citation that used to embed a raw line number
(`garage:1234`, brittle: wrong the moment a line is inserted above it) into a
symbol-based citation instead. test_no_line_number_citations is the
regression guard for that rewrite -- a new `garage:N` citation is that habit
coming back.

Extraction here is pattern-driven, not a fixed inventory of today's
citations: docs/ARCHITECTURE.md in particular is edited often, and a test
that hard-codes "these are the citations that exist" would stop checking
anything the moment the doc changes.
"""

from __future__ import annotations

import re
import unittest

from harness import BACKEND_PATH, BINDS_LUA, REPO_ROOT


DOCS_DIR = REPO_ROOT / "docs"
HYPR_DIR = REPO_ROOT / "desktop" / ".config" / "hypr"
HYPRLAND_LUA = HYPR_DIR / "hyprland.lua"

# `name()` citations that name a real function but not a backend `def` --
# they live on the Lua side of the Python/Hyprland split. Routed to the file
# that actually defines them so the assertion checks the right source instead
# of being silently excused.
LUA_FUNCTION_HOMES = {
    "bind": BINDS_LUA,               # config/binds.lua: `local function bind(...)`
    "load_plugin": HYPRLAND_LUA,     # hyprland.lua: `local function load_plugin(...)`
    "load_override": HYPRLAND_LUA,   # hyprland.lua: `local function load_override(...)`
}

# `name()` citations that are not this repo's code at all -- language
# builtins the docs mention by name because the backend/Lua calls them.
BUILTIN_FUNCTIONS = {
    # Lua's stdlib `dofile`, called from hyprland.lua and config/variables.lua
    # but never itself defined anywhere in this repo.
    "dofile",
}

# ALL_CAPS citations (checked against ARCHITECTURE.md only, see
# test_every_cited_constant_exists) that are real symbols but live in Lua
# rather than as a Python module-level constant.
LUA_CONSTANT_HOMES = {
    "GLASS_AVAILABLE": HYPRLAND_LUA,     # hyprland.lua: `GLASS_AVAILABLE = load_plugin(...)`
    "HYPREXPO_AVAILABLE": HYPRLAND_LUA,  # hyprland.lua: `HYPREXPO_AVAILABLE = load_plugin(...)`
    "RESCUE": BINDS_LUA,                 # config/binds.lua: `local RESCUE = { ... }`
}

# ALL_CAPS citations that are real symbols but are neither a backend
# module-level constant nor a Lua one -- each entry explains where the name
# actually comes from so this stays a short, justified list rather than a
# place to dump anything the other two checks reject.
CONSTANT_ALLOWLIST = {
    "LOCK_NB": "fcntl.LOCK_NB -- a Python stdlib fcntl flag used inline as "
               "`fcntl.LOCK_EX | fcntl.LOCK_NB`, not assigned as a "
               "module-level name anywhere in this repo.",
}

FUNCTION_CITATION = re.compile(r"`([a-z_][a-zA-Z0-9_]*)\(\)`")
CONSTANT_CITATION = re.compile(r"`([A-Z][A-Z0-9_]{2,})`")
LINE_NUMBER_CITATION = re.compile(r"garage:\d+")

# Three surface forms actually in use for a Lua source-line citation:
#   `config/binds.lua:204`               -- single line, one backtick span
#   `config/binds.lua:511`-`519`         -- range split across two spans
#   `file.lua:511-519`                   -- range in one span (documented form)
# Alternation is tried in this order at each position, so the split-range
# form (which a plain single-line match would otherwise partially match) is
# checked first.
LUA_LINE_CITATION = re.compile(
    r"`([\w./-]+\.lua):(\d+)`-`(\d+)`"
    r"|`([\w./-]+\.lua):(\d+)-(\d+)`"
    r"|`([\w./-]+\.lua):(\d+)`"
)


def doc_files():
    return sorted(DOCS_DIR.glob("*.md"))


def cited_symbols(pattern):
    """name -> sorted doc filenames that cite it, across every docs/*.md."""
    hits: dict[str, set[str]] = {}
    for path in doc_files():
        for match in pattern.finditer(path.read_text()):
            hits.setdefault(match.group(1), set()).add(path.name)
    return hits


class DocCitations(unittest.TestCase):
    def test_no_line_number_citations(self) -> None:
        """No doc cites code by raw `garage:N` line number.

        commit 674a56e replaced every such citation with a symbol-based one:
        a line number is wrong the instant a line is inserted above it, so
        keeping any is a citation quietly rotting from day one.
        """
        offenders = []
        for path in doc_files():
            for match in LINE_NUMBER_CITATION.finditer(path.read_text()):
                offenders.append(f"{path.name}: {match.group(0)}")
        self.assertEqual([], sorted(offenders))

    def test_every_cited_backend_symbol_exists(self) -> None:
        """Every `name()` citation resolves to a real function.

        Most resolve as `def name` in the Python backend. A handful of names
        are cited that live on the Lua side of the split (keybind wiring,
        plugin loading) or are language builtins mentioned by name; those are
        routed via LUA_FUNCTION_HOMES / BUILTIN_FUNCTIONS instead of being
        checked against the backend. A name in neither the backend nor either
        of those maps is a citation of something that does not exist.
        """
        backend = BACKEND_PATH.read_text()
        lua_text = {home: home.read_text() for home in set(LUA_FUNCTION_HOMES.values())}

        missing = []
        for name, docs in sorted(cited_symbols(FUNCTION_CITATION).items()):
            where = ", ".join(sorted(docs))
            if name in BUILTIN_FUNCTIONS:
                continue
            if name in LUA_FUNCTION_HOMES:
                home = LUA_FUNCTION_HOMES[name]
                if not re.search(rf"^(local )?function {re.escape(name)}\b",
                                  lua_text[home], re.MULTILINE):
                    missing.append(f"{name}() (cited in {where}) -- no "
                                    f"`function {name}` in {home}")
                continue
            if not re.search(rf"^def {re.escape(name)}\b", backend, re.MULTILINE):
                missing.append(f"{name}() (cited in {where}) -- no "
                                f"`def {name}` in {BACKEND_PATH}")
        self.assertEqual([], missing)

    def test_every_cited_constant_exists(self) -> None:
        """Every ALL_CAPS `NAME` citation in ARCHITECTURE.md is a real constant.

        Restricted to ARCHITECTURE.md, not every doc: that is the doc that
        describes data flow through named constants, so it is where such
        citations accumulate. Running the same all-caps pattern over every
        doc would also catch plain acronyms in prose (ABI, TOML, JSON, GTK,
        QML, ...) that are not code citations at all.

        Most resolve as a module-level assignment (`NAME = ...` or
        `NAME: ... = ...`) in the Python backend. A few are Lua-side
        (LUA_CONSTANT_HOMES) or real symbols that are not a module-level
        assignment anywhere in this repo (CONSTANT_ALLOWLIST, each entry
        explaining where the name actually comes from).
        """
        path = DOCS_DIR / "ARCHITECTURE.md"
        text = path.read_text()
        backend = BACKEND_PATH.read_text()
        lua_text = {home: home.read_text() for home in set(LUA_CONSTANT_HOMES.values())}

        names: dict[str, str] = {}
        for match in CONSTANT_CITATION.finditer(text):
            names.setdefault(match.group(1), path.name)

        missing = []
        for name in sorted(names):
            if name in CONSTANT_ALLOWLIST:
                continue
            if name in LUA_CONSTANT_HOMES:
                home = LUA_CONSTANT_HOMES[name]
                if not re.search(rf"^(local )?{re.escape(name)}\s*=",
                                  lua_text[home], re.MULTILINE):
                    missing.append(f"{name} (cited in {names[name]}) -- not "
                                    f"assigned in {home}")
                continue
            if not re.search(rf"^{re.escape(name)}\s*[:=]", backend, re.MULTILINE):
                missing.append(f"{name} (cited in {names[name]}) -- no "
                                f"module-level `{name} =` / `{name}:` in {BACKEND_PATH}")
        self.assertEqual([], missing)

    def test_lua_line_citations_resolve(self) -> None:
        """Every `file.lua:N` (or `:N-M`) citation names a real, in-range line.

        Resolved by suffix match against every `*.lua` under
        desktop/.config/hypr, not by literal path: docs cite paths relative
        to the Hyprland config directory (`hyprland.lua`, `config/binds.lua`),
        which is not the checkout-relative path. A citation that resolves to
        zero or more than one file is itself a failure -- either the file
        does not exist under that tree, or the cited path is too short to
        pick a single file out of it.
        """
        lua_files = list(HYPR_DIR.rglob("*.lua"))

        def resolve(cited_path: str):
            return [f for f in lua_files
                    if f.name == cited_path or str(f).endswith("/" + cited_path)]

        failures = []
        for path in doc_files():
            for match in LUA_LINE_CITATION.finditer(path.read_text()):
                if match.group(1) is not None:
                    cited, start, end = match.group(1), int(match.group(2)), int(match.group(3))
                elif match.group(4) is not None:
                    cited, start, end = match.group(4), int(match.group(5)), int(match.group(6))
                else:
                    cited, start, end = match.group(7), int(match.group(8)), None

                targets = resolve(cited)
                if len(targets) != 1:
                    verdict = "no file" if not targets else "ambiguous file"
                    failures.append(f"{path.name}: {match.group(0)} -- {verdict} "
                                     f"resolves under {HYPR_DIR} for {cited!r}")
                    continue

                line_count = len(targets[0].read_text().splitlines())
                for value in (start, end):
                    if value is not None and value > line_count:
                        failures.append(
                            f"{path.name}: {match.group(0)} -- line {value} exceeds "
                            f"{targets[0]} ({line_count} lines)")
        self.assertEqual([], sorted(failures))


if __name__ == "__main__":
    unittest.main()
