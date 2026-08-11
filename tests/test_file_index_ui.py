"""System Preferences exposes file indexing without hard-coded window geometry."""

from pathlib import Path
import unittest


REPO_ROOT = Path(__file__).resolve().parent.parent
GARAGE_QML = REPO_ROOT / "desktop" / ".config" / "quickshell" / "garage"


class FileIndexPreferencesUi(unittest.TestCase):
    def test_general_pane_orders_launcher_search_indexing_then_defaults(self) -> None:
        pane = (GARAGE_QML / "GeneralPane.qml").read_text(encoding="utf-8")
        positions = [pane.index(f'title: "{title}"') for title in (
            "LAUNCHER", "SEARCH", "FILE INDEXING", "DEFAULT APPLICATIONS")]
        self.assertEqual(sorted(positions), positions)
        for text in ("Index Activity", "Last Index", "Refresh Now"):
            self.assertIn(text, pane)

    def test_preferences_height_and_about_follow_sidebar_content(self) -> None:
        palette = (GARAGE_QML / "PreferencesPalette.qml").read_text(encoding="utf-8")
        self.assertIn("sidebarContent.implicitHeight", palette)
        self.assertIn("+ sidebarTopMargin + sidebarBottomMargin", palette)
        self.assertIn("minimumSize: Qt.size(560, Math.ceil(", palette)
        self.assertIn("model: preferences.categories", palette)
        self.assertNotIn("categories.filter", palette)
        self.assertEqual(1, palette.count('title: "About"'))

    def test_folder_picker_and_status_processes_are_wired(self) -> None:
        controller = (GARAGE_QML / "PreferencesController.qml").read_text(
            encoding="utf-8")
        palette = (GARAGE_QML / "PreferencesPalette.qml").read_text(encoding="utf-8")
        self.assertIn('command: [controller.indexHelper, "status"]', controller)
        self.assertIn('command: [controller.indexHelper, "refresh"]', controller)
        self.assertIn("IndexDirectoryPicker {", palette)

if __name__ == "__main__":
    unittest.main()
