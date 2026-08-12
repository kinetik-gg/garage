"""Garage owns the polkit prompt surface without replacing polkit itself."""

from __future__ import annotations

from pathlib import Path
import unittest


REPO_ROOT = Path(__file__).resolve().parent.parent
QML = REPO_ROOT / "system" / "hyprpolkitagent" / "main.qml"
BUILD = REPO_ROOT / "system" / "hyprpolkitagent" / "build"
LICENSE = REPO_ROOT / "system" / "hyprpolkitagent" / "LICENSE"
DROPIN = (
    REPO_ROOT / "desktop" / ".config" / "systemd" / "user"
    / "hyprpolkitagent.service.d" / "garage.conf"
)
BOOTSTRAP = REPO_ROOT / "bootstrap.sh"


class AuthenticationModal(unittest.TestCase):
    def setUp(self) -> None:
        self.qml = QML.read_text(encoding="utf-8")

    def test_geometry_and_typography_are_bounded_not_font_metric_multiplied(self) -> None:
        self.assertIn("width: 560", self.qml)
        self.assertIn("Math.max(320, Math.min(420", self.qml)
        self.assertIn('font.family: "Plus Jakarta Sans"', self.qml)
        self.assertIn("font.pixelSize: 21", self.qml)
        self.assertNotIn("FontMetrics", self.qml)

    def test_request_is_compact_but_keeps_the_action_context(self) -> None:
        self.assertIn("text: window.agent.getMessage()", self.qml)
        self.assertIn("maximumLineCount: 3", self.qml)
        self.assertIn("Text.Wrap", self.qml)
        self.assertIn('text: "Authorize as "', self.qml)

    def test_password_goes_directly_to_upstream_and_is_never_logged(self) -> None:
        self.assertIn('agent.setResult("auth:" + passwordField.text)', self.qml)
        self.assertIn("TextInput.Password", self.qml)
        self.assertIn('agent.setResult("fail")', self.qml)
        self.assertNotIn("console.", self.qml)
        self.assertNotRegex(self.qml, r"Process\s*\{")

    def test_failed_authentication_does_not_move_the_layout(self) -> None:
        error = self.qml.split("id: errorLabel", 1)[1].split("Connections", 1)[0]
        self.assertIn("Layout.preferredHeight: 18", error)
        self.assertIn('text: ""', error)
        self.assertIn("passwordField.selectAll()", self.qml)

    def test_blocked_state_prevents_duplicate_submissions(self) -> None:
        self.assertIn("if (submitted || blocked", self.qml)
        self.assertIn("readOnly: window.blocked", self.qml)
        self.assertIn('window.blocked ? "Authenticating…"', self.qml)
        self.assertIn("enabled: !window.blocked", self.qml)

    def test_modal_uses_the_generated_qt_palette_for_primary_surfaces(self) -> None:
        self.assertIn("SystemPalette", self.qml)
        for role in ("system.window", "system.windowText", "system.base",
                     "system.alternateBase", "system.highlight"):
            with self.subTest(role=role):
                self.assertIn(role, self.qml)


class AuthenticationIntegration(unittest.TestCase):
    def test_build_is_checksum_pinned_to_the_packaged_upstream_release(self) -> None:
        build = BUILD.read_text(encoding="utf-8")
        self.assertIn("version=0.1.3", build)
        self.assertIn(
            "a8fa714b92d47331f056b608cb731dd1f5cc3845a9109cb22c6e6eb55b4eac84",
            build,
        )
        self.assertIn("sha256sum --check --status", build)
        self.assertIn("hyprwm/hyprpolkitagent/archive", build)
        self.assertIn('install -m 0644 "$script_dir/main.qml"', build)
        self.assertIn('mv -f -- "$target.new" "$target"', build)

    def test_service_overrides_only_the_agent_executable_and_control_style(self) -> None:
        dropin = DROPIN.read_text(encoding="utf-8")
        self.assertRegex(dropin, r"(?m)^ExecStart=$")
        self.assertIn("ExecStart=%h/.local/lib/garage/hyprpolkitagent", dropin)
        self.assertIn("Environment=QT_QUICK_CONTROLS_STYLE=Basic", dropin)

    def test_bootstrap_keeps_the_distro_backend_and_builds_the_garage_surface(self) -> None:
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        package_block = bootstrap.split("packages=(", 1)[1].split("\n)", 1)[0]
        self.assertRegex(package_block, r"\bhyprpolkitagent\b")
        self.assertRegex(package_block, r"\bcurl\b")
        self.assertIn('system/hyprpolkitagent/build"', bootstrap)
        self.assertIn('"$HOME/.local/lib/garage/hyprpolkitagent"', bootstrap)

    def test_upstream_license_is_retained_with_the_derivative_build(self) -> None:
        license_text = LICENSE.read_text(encoding="utf-8")
        self.assertIn("BSD 3-Clause License", license_text)
        self.assertIn("Copyright (c) 2024, Hypr Development", license_text)


if __name__ == "__main__":
    unittest.main()
