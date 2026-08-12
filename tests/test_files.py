"""Thunar is Garage's cohesive, capable file explorer rather than a GTK extra."""

from __future__ import annotations

import configparser
from pathlib import Path
import re
import unittest
import xml.etree.ElementTree as ET


REPO_ROOT = Path(__file__).resolve().parent.parent
THUNAR_CSS = REPO_ROOT / "desktop" / ".config" / "gtk-3.0" / "thunar.css"
MIMEAPPS = REPO_ROOT / "desktop" / ".config" / "mimeapps.list"
THUNAR_DEFAULTS = REPO_ROOT / "templates" / "thunar.xml"
BOOTSTRAP = REPO_ROOT / "bootstrap.sh"


class ThunarTheme(unittest.TestCase):
    def setUp(self) -> None:
        self.css = THUNAR_CSS.read_text(encoding="utf-8")

    def test_every_rule_is_scoped_to_thunar(self) -> None:
        structural = re.sub(r"/\*.*?\*/", "", self.css, flags=re.DOTALL)
        for group in re.findall(r"([^{}]+)\{", structural):
            for selector in group.split(","):
                with self.subTest(selector=selector.strip()):
                    self.assertIn("thunar", selector.lower())

    def test_structure_uses_the_generated_palette_only(self) -> None:
        self.assertIsNone(re.search(r"#[0-9a-fA-F]{3,8}\b", self.css))
        self.assertIsNone(re.search(r"\brgba?\(\s*\d", self.css))
        for role in ("@window_bg_color", "@sidebar_bg_color", "@view_bg_color",
                     "@accent_bg_color", "@sidebar_border_color"):
            with self.subTest(role=role):
                self.assertIn(role, self.css)

    def test_all_primary_surfaces_have_an_explicit_hierarchy(self) -> None:
        for selector in ("headerbar", ".location-bar", ".shortcuts-pane",
                         ".standard-view", "notebook > header", ".preview-pane",
                         "statusbar", "scrollbar slider"):
            with self.subTest(selector=selector):
                self.assertIn(selector, self.css)

    def test_sidebar_and_file_rows_have_real_treeview_spacing(self) -> None:
        self.assertIn("-GtkTreeView-vertical-separator: 10px", self.css)
        self.assertIn("-GtkTreeView-horizontal-separator: 14px", self.css)
        self.assertIn(".shortcuts-pane scrolledwindow", self.css)
        self.assertIn("margin: 12px 10px", self.css)

    def test_paned_drag_target_does_not_render_as_a_thick_border(self) -> None:
        separator = self.css.split(".thunar paned > separator", 1)[1].split("}", 1)[0]
        self.assertIn("background: transparent", separator)
        self.assertIn("background-image: none", separator)
        self.assertIn("border: 0", separator)

    def test_sidebar_and_content_meet_without_an_artificial_gutter(self) -> None:
        shortcuts = self.css.split(".thunar .shortcuts-pane {", 1)[1].split("}", 1)[0]
        self.assertNotIn("border-right", shortcuts)
        self.assertNotIn(".standard-view scrolledwindow {", self.css)

    def test_statusbar_surface_is_full_width_but_text_remains_padded(self) -> None:
        statusbar = self.css.split(".thunar statusbar {", 1)[1].split("}", 1)[0]
        self.assertIn("padding: 0", statusbar)
        self.assertIn(".thunar statusbar box", self.css)
        label = self.css.split(".thunar statusbar label {", 1)[1].split("}", 1)[0]
        self.assertIn("padding: 7px 14px", label)


class ThunarIntegration(unittest.TestCase):
    def test_thunar_owns_folder_and_network_locations(self) -> None:
        parser = configparser.ConfigParser(interpolation=None)
        parser.read(MIMEAPPS, encoding="utf-8")
        defaults = parser["Default Applications"]
        self.assertEqual("thunar.desktop", defaults["inode/directory"])
        self.assertEqual("thunar.desktop", defaults["x-scheme-handler/smb"])
        self.assertNotIn("org.gnome.Nautilus.desktop", defaults.values())

    def test_bootstrap_installs_the_complete_thunar_stack(self) -> None:
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        package_block = bootstrap.split("packages=(", 1)[1].split("\n)", 1)[0]
        for package in ("thunar", "thunar-archive-plugin",
                        "thunar-media-tags-plugin", "tumbler", "catfish",
                        "file-roller", "ffmpegthumbnailer", "gvfs-smb"):
            with self.subTest(package=package):
                self.assertRegex(package_block, rf"\b{re.escape(package)}\b")
        self.assertNotRegex(package_block, r"\bnautilus\b")

    def test_first_run_defaults_are_copied_but_never_linked(self) -> None:
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        self.assertIn('cp -- "$repo_dir/templates/thunar.xml" "$thunar_config"',
                      bootstrap)
        self.assertIn("keeping the existing Thunar layout", bootstrap)
        self.assertFalse(str(THUNAR_DEFAULTS.relative_to(REPO_ROOT)).startswith("desktop/"))

    def test_first_run_layout_is_modern_without_hiding_capability(self) -> None:
        root = ET.parse(THUNAR_DEFAULTS).getroot()
        properties = {item.attrib["name"]: item.attrib["value"]
                      for item in root.findall("property")}
        self.assertEqual("true", properties["misc-use-csd"])
        self.assertEqual("true", properties["misc-symbolic-icons-in-toolbar"])
        self.assertEqual("true", properties["misc-symbolic-icons-in-sidepane"])
        self.assertEqual("true", properties["last-statusbar-visible"])
        toolbar = properties["last-toolbar-items"]
        self.assertEqual("THUNAR_ZOOM_LEVEL_50_PERCENT",
                         properties["last-details-view-zoom-level"])
        visible = ("back:1", "forward:1", "location-bar:1",
                   "view-switcher:1", "search:1", "menu:1")
        hidden = ("open-parent:0", "open-home:0", "toggle-split-view:0")
        for item in visible + hidden:
            with self.subTest(item=item):
                self.assertIn(item, toolbar)
        configured = toolbar.split(",")
        self.assertEqual(list(visible),
                         [item for item in configured if item.endswith(":1")])


if __name__ == "__main__":
    unittest.main()
