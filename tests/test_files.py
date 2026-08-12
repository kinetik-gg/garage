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
GARAGE_BACKEND = REPO_ROOT / "desktop" / ".local" / "bin" / "garage"
THUNAR_MODULE = REPO_ROOT / "system" / "gtk-modules" / "garage-thunar.c"
UWSM_ENV = REPO_ROOT / "desktop" / ".config" / "uwsm" / "env"
HYPR_VARIABLES = REPO_ROOT / "desktop" / ".config" / "hypr" / "config" / "variables.lua"
FILES_OPENER = REPO_ROOT / "desktop" / ".local" / "bin" / "garage-open-files"
THUNAR_WRAPPER = REPO_ROOT / "desktop" / ".local" / "bin" / "thunar"
THUNAR_SYSTEMD_DROPIN = (
    REPO_ROOT / "desktop" / ".config" / "systemd" / "user"
    / "thunar.service.d" / "garage.conf"
)


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
                         ".garage-tree-pane",
                         ".standard-view", "notebook > header", ".preview-pane",
                         "statusbar", "scrollbar slider"):
            with self.subTest(selector=selector):
                self.assertIn(selector, self.css)

    def test_sidebar_and_file_rows_have_real_treeview_spacing(self) -> None:
        self.assertIn("-GtkTreeView-vertical-separator: 10px", self.css)
        self.assertIn("-GtkTreeView-horizontal-separator: 14px", self.css)
        self.assertIn(".shortcuts-pane scrolledwindow", self.css)
        shortcuts = self.css.split(".thunar .shortcuts-pane {", 1)[1].split("}", 1)[0]
        self.assertIn("padding: 12px 10px", shortcuts)

    def test_sidebar_headers_do_not_receive_the_item_hover_surface(self) -> None:
        neutral = self.css.split(
            ".thunar .shortcuts-pane treeview.view:hover:not(:selected)", 1
        )[1].split("}", 1)[0]
        self.assertIn("background-color: transparent", neutral)
        self.assertIn("garage-shortcut-item-hover", self.css)

    def test_tree_sidebar_has_its_own_padding_and_row_treatment(self) -> None:
        tree_pane = self.css.split(
            ".thunar .garage-tree-pane {", 1
        )[1].split("}", 1)[0]
        self.assertIn("padding: 12px 10px", tree_pane)
        self.assertIn("treeview.view.garage-tree-view", self.css)
        self.assertIn(
            ".garage-tree-pane treeview.view.garage-tree-view:hover:not(:selected)",
            self.css,
        )
        self.assertIn(
            ".garage-tree-pane treeview.view.garage-tree-view expander", self.css
        )
        self.assertIn(".sidebar:not(.shortcuts-pane) > treeview.view", self.css)
        self.assertNotIn("treeview.sidebar", self.css)

    def test_paned_handle_reserves_no_visual_gutter(self) -> None:
        paned = self.css.split(".thunar paned {", 1)[1].split("}", 1)[0]
        separator = self.css.split(".thunar paned > separator", 1)[1].split("}", 1)[0]
        self.assertIn("-GtkPaned-handle-size: 0", paned)
        self.assertIn("min-width: 0", separator)
        self.assertIn("margin: 0", separator)
        self.assertIn("padding: 0", separator)
        self.assertIn("background-color: transparent", separator)
        self.assertIn("background-image: none", separator)
        self.assertIn("border: 0", separator)

    def test_sidebar_and_content_meet_without_an_artificial_gutter(self) -> None:
        shortcuts = self.css.split(".thunar .shortcuts-pane {", 1)[1].split("}", 1)[0]
        self.assertNotIn("border-right", shortcuts)
        self.assertNotIn(".standard-view scrolledwindow {", self.css)

    def test_toolbar_matches_the_column_header_without_a_divider(self) -> None:
        toolbar = self.css.split(
            ".thunar .garage-content-shell > toolbar {", 1
        )[1].split("}", 1)[0]
        header = self.css.split(
            ".thunar treeview.view header button {", 1
        )[1].split("}", 1)[0]
        self.assertIn("background-color: alpha(@view_bg_color, 0.98)", toolbar)
        self.assertIn("background-color: alpha(@view_bg_color, 0.98)", header)
        self.assertNotIn("border-bottom", toolbar)
        self.assertNotIn("garage-sidebar-header", self.css)

    def test_address_keeps_native_segments_without_internal_controls(self) -> None:
        breadcrumb = self.css.split(
            ".thunar .garage-breadcrumb {", 1
        )[1].split("}", 1)[0]
        self.assertIn("background: transparent", breadcrumb)
        self.assertIn("border: 0", breadcrumb)

    def test_statusbar_surface_is_full_width_but_text_remains_padded(self) -> None:
        statusbar = self.css.split(".thunar statusbar {", 1)[1].split("}", 1)[0]
        self.assertIn("padding: 0", statusbar)
        self.assertIn("margin: 0 -10px", statusbar)
        self.assertIn(".thunar statusbar box", self.css)
        label = self.css.split(".thunar statusbar label {", 1)[1].split("}", 1)[0]
        self.assertIn("padding: 7px 14px", label)


class ThunarIntegration(unittest.TestCase):
    def test_gtk_entrypoint_contains_only_the_thunar_file_manager_sheet(self) -> None:
        backend = GARAGE_BACKEND.read_text(encoding="utf-8")
        self.assertIn('@import url("thunar.css");', backend)
        self.assertNotIn("pantheon-files.css", backend)
        self.assertNotIn("nautilus.css", backend)
        self.assertFalse(
            (REPO_ROOT / "desktop" / ".config" / "gtk-3.0"
             / "pantheon-files.css").exists()
        )
        self.assertFalse(
            (REPO_ROOT / "desktop" / ".config" / "gtk-4.0"
             / "nautilus.css").exists()
        )

    def test_thunar_owns_folder_and_network_locations(self) -> None:
        parser = configparser.ConfigParser(interpolation=None)
        parser.read(MIMEAPPS, encoding="utf-8")
        defaults = parser["Default Applications"]
        self.assertEqual("thunar.desktop", defaults["inode/directory"])
        self.assertEqual("thunar.desktop", defaults["x-scheme-handler/smb"])
        self.assertNotIn("org.gnome.Nautilus.desktop", defaults.values())

    def test_file_manager_binding_resolves_the_xdg_default_at_launch(self) -> None:
        variables = HYPR_VARIABLES.read_text(encoding="utf-8")
        opener = FILES_OPENER.read_text(encoding="utf-8")
        self.assertIn("garage-open-files", variables)
        self.assertNotIn("nautilus", variables.lower())
        self.assertIn('xdg-open "${1:-$HOME}"', opener)

    def test_thunar_module_supplies_structure_gtk_css_cannot_express(self) -> None:
        module = THUNAR_MODULE.read_text(encoding="utf-8")
        environment = UWSM_ENV.read_text(encoding="utf-8")
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        self.assertIn("garage-shortcut-item-hover", module)
        self.assertIn("ThunarTreePane", module)
        self.assertIn("ThunarTreeView", module)
        self.assertIn("garage-tree-pane", module)
        self.assertIn("garage-tree-view", module)
        self.assertIn("gtk_tree_view_set_level_indentation", module)
        self.assertIn("gtk_tree_model_get_column_type", module)
        self.assertIn("garage_details_draw_stripes", module)
        self.assertIn("gtk_paned_pack2", module)
        self.assertIn("garage-content-shell", module)
        self.assertIn("gtk_container_remove", module)
        self.assertIn("gtk_menu_button_get_popup", module)
        self.assertIn("garage_install_breadcrumb", module)
        self.assertIn("entry-requested", module)
        self.assertIn("ThunarLocationButton", module)
        self.assertIn("gtk_container_forall", module)
        self.assertIn('g_object_set (controller, "current-directory", NULL', module)
        self.assertIn("garage_center_statusbar", module)
        self.assertIn("gtk_widget_set_margin_start (widget, 14)", module)
        self.assertIn("gtk_widget_set_margin_end (widget, 14)", module)
        self.assertNotIn("thunarx_file_info_get_location", module)
        self.assertNotIn("io.elementary.files", module)
        self.assertNotIn("garage-pantheon-files", module)
        self.assertIn("garage-thunar.so", environment)
        self.assertIn("system/gtk-modules/garage-thunar.c", bootstrap)

    def test_every_thunar_launch_path_loads_the_integration(self) -> None:
        wrapper = THUNAR_WRAPPER.read_text(encoding="utf-8")
        dropin = THUNAR_SYSTEMD_DROPIN.read_text(encoding="utf-8")
        self.assertIn("garage-thunar.so", wrapper)
        self.assertIn('exec /usr/bin/thunar "$@"', wrapper)
        self.assertIn("ExecStart=", dropin)
        self.assertIn("ExecStart=%h/.local/bin/thunar --daemon", dropin)

    def test_bootstrap_installs_the_complete_thunar_stack(self) -> None:
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        package_block = bootstrap.split("packages=(", 1)[1].split("\n)", 1)[0]
        for package in ("thunar", "thunar-archive-plugin",
                        "thunar-media-tags-plugin", "tumbler", "catfish",
                        "file-roller", "ffmpegthumbnailer", "poppler-glib",
                        "gvfs-smb"):
            with self.subTest(package=package):
                self.assertRegex(package_block, rf"\b{re.escape(package)}\b")
        self.assertNotRegex(package_block, r"\bnautilus\b")
        self.assertNotRegex(package_block, r"\bpantheon-files\b")

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
        self.assertEqual("ThunarDetailsView", properties["default-view"])
        self.assertEqual("ThunarDetailsView", properties["last-view"])
        self.assertEqual("false", properties["misc-use-csd"])
        self.assertEqual("true", properties["misc-symbolic-icons-in-toolbar"])
        self.assertEqual("true", properties["misc-symbolic-icons-in-sidepane"])
        self.assertEqual("true", properties["last-statusbar-visible"])
        toolbar = properties["last-toolbar-items"]
        self.assertEqual("THUNAR_ZOOM_LEVEL_25_PERCENT",
                         properties["last-details-view-zoom-level"])
        visible = ("back:1", "forward:1", "location-bar:1",
                   "view-switcher:1", "search:1", "menu:1")
        hidden = ("open-parent:0", "open-home:0", "toggle-split-view:0")
        self.assertIn("view-as-compact-list:0", toolbar)
        for item in visible + hidden:
            with self.subTest(item=item):
                self.assertIn(item, toolbar)
        configured = toolbar.split(",")
        self.assertEqual(list(visible),
                         [item for item in configured if item.endswith(":1")])


if __name__ == "__main__":
    unittest.main()
