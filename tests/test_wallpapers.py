"""The bundled wallpaper files stay aligned with their provenance manifest."""

from __future__ import annotations

import hashlib
from pathlib import Path
import tomllib
import unittest


REPO_ROOT = Path(__file__).resolve().parent.parent
WALLPAPER_ROOT = REPO_ROOT / "desktop" / "Wallpaper"
MANIFEST_PATH = WALLPAPER_ROOT / "wallpapers.toml"


class WallpaperManifest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.manifest = tomllib.loads(MANIFEST_PATH.read_text())
        cls.entries = cls.manifest["wallpaper"]

    def test_collection_has_sixteen_unique_images_per_appearance(self) -> None:
        self.assertEqual(32, len(self.entries))
        self.assertEqual(32, len({entry["id"] for entry in self.entries}))
        for appearance in ("dark", "light"):
            self.assertEqual(
                16,
                sum(entry["appearance"] == appearance for entry in self.entries),
            )

    def test_manifest_matches_exactly_the_bundled_images(self) -> None:
        declared = {entry["file"] for entry in self.entries}
        bundled = {
            str(path.relative_to(WALLPAPER_ROOT))
            for appearance in ("Dark", "Light")
            for path in (WALLPAPER_ROOT / appearance).glob("*.jpg")
        }
        self.assertEqual(declared, bundled)

    def test_files_match_recorded_checksums(self) -> None:
        for entry in self.entries:
            path = WALLPAPER_ROOT / entry["file"]
            with self.subTest(wallpaper=entry["id"]):
                self.assertEqual(
                    entry["sha256"],
                    hashlib.sha256(path.read_bytes()).hexdigest(),
                )

    def test_every_entry_has_unsplash_attribution(self) -> None:
        for entry in self.entries:
            with self.subTest(wallpaper=entry["id"]):
                self.assertTrue(entry["creator_name"])
                self.assertEqual(
                    f"https://unsplash.com/@{entry['creator_username']}",
                    entry["creator_url"],
                )
                self.assertTrue(entry["source_url"].startswith(
                    "https://unsplash.com/photos/"
                ))
                self.assertTrue(entry["source_url"].endswith(entry["id"]))


if __name__ == "__main__":
    unittest.main()
