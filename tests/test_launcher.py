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

    def test_query_results_are_debounced_and_committed_after_delegate_creation(self) -> None:
        self.assertIn("id: rebuildTimer", self.qml)
        self.assertIn("interval: 55", self.qml)
        self.assertIn("id: geometryCommit", self.qml)
        self.assertIn("resultList.itemAtIndex(last) === null", self.qml)
        self.assertLess(
            self.qml.index("launcher.pendingRowCount = rows.length"),
            self.qml.index("launcher.rowCount = launcher.pendingRowCount"))

    def test_field_has_an_explicit_origin_outside_vertical_layout(self) -> None:
        body_start = self.qml.index("id: body")
        list_end = self.qml.index("delegate: LauncherResult", body_start)
        body = self.qml[body_start:list_end]

        self.assertIn("anchors.top: parent.top", body)
        self.assertIn("id: fieldRow", body)
        self.assertIn("height: launcher.fieldHeight", body)
        self.assertIn("anchors.top: fieldRow.bottom", body)
        self.assertNotIn("ColumnLayout {", body)


class LauncherGlassRouting(unittest.TestCase):
    def test_fixed_host_is_not_a_liquid_glass_surface(self) -> None:
        decorations = DECORATIONS.read_text(encoding="utf-8")
        line = next(line for line in decorations.splitlines()
                    if "layer_namespaces =" in line)

        self.assertIn("garage-launcher-glass", line)
        self.assertNotIn("garage-launcher-host", line)

    def test_legacy_surface_keeps_glass_during_a_staggered_reload(self) -> None:
        decorations = DECORATIONS.read_text(encoding="utf-8")
        line = next(line for line in decorations.splitlines()
                    if "layer_namespaces =" in line)

        self.assertIn("garage-launcher,", line)

    def test_backing_gets_blur_and_both_surfaces_disable_compositor_motion(self) -> None:
        rules = WINDOW_RULES.read_text(encoding="utf-8")
        blur_start = rules.index('name = "apple-dark-shell-blur"')
        blur_end = rules.index("})", blur_start)
        blur_rule = rules[blur_start:blur_end]
        blur_match = next(line for line in blur_rule.splitlines()
                          if "match =" in line)
        motion_start = rules.index('name = "static-shell-layers"')
        motion_end = rules.index("})", motion_start)
        motion_rule = rules[motion_start:motion_end]

        self.assertIn("garage-launcher-glass", blur_rule)
        self.assertNotIn("garage-launcher-host", blur_match)
        self.assertIn("|garage-launcher|", motion_rule)
        self.assertIn("|garage-launcher-host|", motion_rule)
        self.assertIn("|garage-launcher-glass|", motion_rule)


if __name__ == "__main__":
    unittest.main()
