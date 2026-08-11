"""Structural regressions for the two-surface launcher.

The launcher changes its visible height as a query gains and loses results.  Its
interactive layer must not resize with that panel: that was the last changing
geometry shared by the field and delegates.  Kinetik Glass, meanwhile, paints a
layer's complete box regardless of client alpha, so the fixed transparent area
cannot itself be the glass layer.  These tests pin that split at both ends.
"""

from __future__ import annotations

from pathlib import Path
import unittest


REPO_ROOT = Path(__file__).resolve().parent.parent
CONFIG = REPO_ROOT / "desktop" / ".config"
LAUNCHER = CONFIG / "quickshell" / "garage" / "LauncherPalette.qml"
DECORATIONS = CONFIG / "hypr" / "config" / "decorations.lua"
WINDOW_RULES = CONFIG / "hypr" / "config" / "windowrules.lua"


class StableLauncherSurface(unittest.TestCase):
    def setUp(self) -> None:
        self.qml = LAUNCHER.read_text(encoding="utf-8")

    def test_interactive_surface_height_has_no_result_dependency(self) -> None:
        start = self.qml.index("readonly property real surfaceHeight:")
        end = self.qml.index("readonly property real bodyHeight:", start)
        definition = self.qml[start:end]

        self.assertIn("launcher.maxRows * launcher.rowHeight", definition)
        for volatile in ("rowCount", "results", "body.implicitHeight"):
            with self.subTest(volatile=volatile):
                self.assertNotIn(volatile, definition)
        self.assertIn("implicitHeight: launcher.surfaceHeight", self.qml)

    def test_only_visible_geometry_tracks_content_height(self) -> None:
        self.assertIn("implicitHeight: launcher.contentHeight", self.qml)
        self.assertIn("height: launcher.contentHeight", self.qml)
        self.assertIn("mask: Region { item: panel }", self.qml)
        self.assertIn("mask: Region {}", self.qml)

    def test_glass_backing_cannot_take_input(self) -> None:
        start = self.qml.index("id: glassSurface")
        end = self.qml.index("// The search engine", start)
        glass = self.qml[start:end]

        self.assertIn("focusable: false", glass)
        self.assertIn("WlrLayershell.layer: WlrLayer.Top", glass)
        self.assertIn(
            "WlrLayershell.keyboardFocus: WlrKeyboardFocus.None", glass)


class LauncherGlassRouting(unittest.TestCase):
    def test_only_content_sized_backing_gets_liquid_glass(self) -> None:
        decorations = DECORATIONS.read_text(encoding="utf-8")
        line = next(line for line in decorations.splitlines()
                    if "layer_namespaces =" in line)

        self.assertIn("garage-launcher-glass", line)
        self.assertNotIn("garage-launcher,", line)

    def test_backing_gets_blur_and_both_surfaces_disable_compositor_motion(self) -> None:
        rules = WINDOW_RULES.read_text(encoding="utf-8")
        blur_start = rules.index('name = "apple-dark-shell-blur"')
        blur_end = rules.index("})", blur_start)
        blur_rule = rules[blur_start:blur_end]
        motion_start = rules.index('name = "static-shell-layers"')
        motion_end = rules.index("})", motion_start)
        motion_rule = rules[motion_start:motion_end]

        self.assertIn("garage-launcher-glass", blur_rule)
        self.assertNotIn("|garage-launcher|", blur_rule)
        self.assertIn("|garage-launcher|", motion_rule)
        self.assertIn("|garage-launcher-glass|", motion_rule)


if __name__ == "__main__":
    unittest.main()
