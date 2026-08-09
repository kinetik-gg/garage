"""Loading the `garage` backend into a test process, safely.

Two things make this awkward, and neither is a thing the tests are allowed to
change:

  1. `desktop/.local/bin/garage` is an extensionless script, not an importable
     module. `SourceFileLoader` is given the path directly, under a module name
     that is not "__main__" so the `if __name__ == "__main__"` tail does not
     run main().
  2. Every path the backend touches is bound at import time from `Path.home()`
     and the XDG / GARAGE_* environment. So the environment has to be scratch
     *before* the module is executed, and a module already loaded against one
     scratch directory cannot be reused for another.

`load_backend()` therefore does a fresh load per call, inside a temporarily
patched environment, and then asserts that every path constant it can see
landed under the scratch root. That assertion is the real guarantee that a test
run cannot write to the developer's own ~/.config/garage -- it fails loudly
rather than quietly reaching outside.
"""

from __future__ import annotations

import contextlib
import importlib.machinery
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest


TESTS_DIR = Path(__file__).resolve().parent
REPO_ROOT = TESTS_DIR.parent
BACKEND_PATH = REPO_ROOT / "desktop" / ".local" / "bin" / "garage"
DEFAULTS_TOML = REPO_ROOT / "desktop" / ".config" / "garage" / "preferences.defaults.toml"
BINDS_LUA = REPO_ROOT / "desktop" / ".config" / "hypr" / "config" / "binds.lua"

# Env vars that would otherwise point the backend at the real desktop. Cleared
# or redirected for every load; HOME is included because HOME is what
# Path.home() reads, and WALLPAPER_DIR/CURRENT_WALLPAPER hang off it directly
# with no override of their own.
_REDIRECTED = ("HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "GARAGE_PREFERENCES",
               "GARAGE_DISPLAYS", "GARAGE_KEYBINDINGS", "GARAGE_WORKSPACE_BLOCKS")

# Counter so each load gets a distinct module name. Same name twice would still
# work (nothing is put in sys.modules) but distinct names make a leaked
# reference obvious in a traceback.
_loads = 0


@contextlib.contextmanager
def scratch_environment(root: Path):
    """Run a block with the backend's whole path environment inside `root`."""
    root = Path(root)
    (root / ".config" / "garage").mkdir(parents=True, exist_ok=True)
    (root / ".local" / "state" / "garage").mkdir(parents=True, exist_ok=True)
    replacement = {
        "HOME": str(root),
        "XDG_CONFIG_HOME": str(root / ".config"),
        "XDG_STATE_HOME": str(root / ".local" / "state"),
    }
    previous = {name: os.environ.get(name) for name in _REDIRECTED}
    try:
        for name in _REDIRECTED:
            os.environ.pop(name, None)
        os.environ.update(replacement)
        yield root
    finally:
        for name, value in previous.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value


def load_backend(root: Path):
    """Execute the backend script fresh, with its paths bound inside `root`."""
    global _loads
    _loads += 1
    name = f"garage_backend_{_loads}"
    with scratch_environment(root):
        loader = importlib.machinery.SourceFileLoader(name, str(BACKEND_PATH))
        spec = importlib.util.spec_from_loader(name, loader)
        module = importlib.util.module_from_spec(spec)
        # Deliberately not registered in sys.modules: a second load with a
        # different scratch root must not be served the first one's paths.
        loader.exec_module(module)
    assert_confined(module, root)
    return module


# Every module-level Path constant, so the confinement check is a whitelist of
# names rather than a guess. Anything new that appears in the backend and is not
# listed here is reported by test_paths_are_enumerated in test_schema.py.
PATH_CONSTANTS = (
    "HOME", "CONFIG_HOME", "STATE_HOME", "ROOT", "STATE_ROOT", "DEFAULTS_PATH",
    "PREFERENCES_PATH", "DISPLAYS_PATH", "KEYBINDINGS_PATH", "WORKSPACE_BLOCKS_PATH",
    "GENERATED", "HYPRLAND_FRAGMENT", "DISPLAY_FRAGMENT", "WORKSPACES_FRAGMENT",
    "KEYBINDS_DATA", "KEYBINDS_CATALOG", "HYPRIDLE_CONFIG", "HYPRPAPER_CONFIG",
    "WALLPAPER_DIR", "CURRENT_WALLPAPER", "ACCENT_FILE", "SCHEME_FILE", "SEARCH_FILE",
    "BAR_FG_FILE", "CORNER_RADIUS_FILE", "MATERIAL_FILE", "LAUNCHER_FILE",
    "TERMINAL_FILE", "BROWSER_FILE", "LOCALE_ENV", "WAYBAR_CLOCK", "WAYBAR_WORKSPACES",
    "GTK3_SETTINGS", "GTK4_SETTINGS", "XSETTINGSD_CONF", "QT6CT_CONF",
    "MIMEAPPS_OVERRIDE", "PENDING_DISPLAY", "DISPLAY_LOCK", "PREFERENCES_LOCK",
    "LEGACY_ROOT",
)

# Paths that are legitimately outside $HOME because they are system locations
# the backend only ever reads. Listed so the enumeration test can tell "a new
# system path" from "a new user path that escaped the redirect", which is the
# one it has to catch.
SYSTEM_PATHS = ("PLUGIN_ROOT",)


def assert_confined(module, root: Path) -> None:
    """Fail unless every path the loaded backend holds is under `root`."""
    root = Path(root).resolve()
    escaped = []
    for name in PATH_CONSTANTS:
        value = getattr(module, name, None)
        if not isinstance(value, Path):
            continue
        try:
            value.resolve().relative_to(root)
        except ValueError:
            escaped.append(f"{name}={value}")
    if escaped:
        raise AssertionError(
            "backend paths escaped the scratch root -- a test run could have "
            f"written to the real desktop: {', '.join(escaped)}")


class BackendTestCase(unittest.TestCase):
    """A test with its own scratch HOME and its own freshly loaded backend."""

    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory(prefix="garage-tests-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.garage = load_backend(self.root)
