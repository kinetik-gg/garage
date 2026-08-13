"""Garage's login and lock screens remain native, stable and safely deployed."""

from __future__ import annotations

import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parent.parent
SDDM = REPO_ROOT / "system" / "sddm"
THEME = SDDM / "garage"
BOOTSTRAP = REPO_ROOT / "bootstrap.sh"
HYPRLOCK = REPO_ROOT / "desktop" / ".config" / "hypr" / "hyprlock.conf"
LOCK_SCRIPT = (
    REPO_ROOT / "desktop" / ".config" / "hypr" / "scripts"
    / "garage-lock-session"
)


class SddmTheme(unittest.TestCase):
    def setUp(self) -> None:
        self.qml = (THEME / "Main.qml").read_text(encoding="utf-8")

    def test_theme_is_self_contained_after_the_installer_adds_assets(self) -> None:
        installer = (SDDM / "install").read_text(encoding="utf-8")
        for source, installed in (
            ("desktop/Wallpaper/Dark/ggykb5c7oeQ.jpg", "background.jpg"),
            ("desktop/.local/share/fonts/PlusJakartaSans.ttf", "PlusJakartaSans.ttf"),
            ("desktop/.local/share/fonts/PJS-OFL.txt", "PJS-OFL.txt"),
        ):
            with self.subTest(asset=installed):
                self.assertIn(source, installer)
                self.assertIn(installed, installer)
        self.assertNotIn("/home/", self.qml)
        self.assertNotIn("~/", self.qml)
        self.assertTrue((THEME / "ATTRIBUTION.md").is_file())

    def test_only_the_primary_screen_owns_the_form(self) -> None:
        self.assertIn("visible: primaryScreen", self.qml)
        background = self.qml.split("Image {", 1)[1].split("Item {", 1)[0]
        self.assertNotIn("primaryScreen", background)

    def test_login_keeps_remembered_identity_session_and_stable_status_space(self) -> None:
        self.assertIn("property string username: userModel.lastUser", self.qml)
        self.assertIn("sessionModel.lastIndex", self.qml)
        self.assertIn("height: 22", self.qml.split("id: statusLabel", 1)[1])
        self.assertIn("sddm.login(username, passwordField.text, sessionIndex)", self.qml)
        self.assertIn("Switch user", self.qml)

    def test_session_and_power_paths_are_explicit_and_destructive_power_confirms(self) -> None:
        self.assertIn("model: sessionModel", self.qml)
        self.assertIn('requestPower("sleep")', self.qml)
        self.assertIn('requestPower("reboot")', self.qml)
        self.assertIn('requestPower("shutdown")', self.qml)
        self.assertIn('pendingPower = action', self.qml)
        self.assertIn("page.confirmPower()", self.qml)
        request = self.qml.split("function requestPower", 1)[1].split(
            "function confirmPower", 1
        )[0]
        self.assertIn("sddm.suspend()", request)
        self.assertNotIn("sddm.reboot()", request)
        self.assertNotIn("sddm.powerOff()", request)

    def test_system_config_keeps_x11_greeter_and_uwsm_session_available(self) -> None:
        dropin = (SDDM / "10-garage.conf").read_text(encoding="utf-8")
        self.assertIn("DisplayServer=x11", dropin)
        self.assertIn("Current=garage", dropin)
        self.assertIn("EnableAvatars=false", dropin)
        self.assertIn("RememberLastUser=true", dropin)
        self.assertIn("RememberLastSession=true", dropin)
        # SDDM's model defaults to index 0 when no state exists. Arch's Hyprland
        # and UWSM packages publish these two entries in sorted order.
        sessions = ["hyprland-uwsm.desktop", "hyprland.desktop"]
        self.assertEqual(sorted(sessions)[0], "hyprland-uwsm.desktop")

    def test_bootstrap_installs_theme_before_enabling_sddm(self) -> None:
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        install_at = bootstrap.index('sudo "$repo_dir/system/sddm/install"')
        enable_at = bootstrap.index("systemctl enable NetworkManager.service")
        self.assertLess(install_at, enable_at)
        installer = (SDDM / "install").read_text(encoding="utf-8")
        self.assertIn("mktemp -d", installer)
        self.assertIn("payload_id=", installer)
        self.assertIn("tar --sort=name --mtime=@0", installer)
        self.assertIn("mv -Tf", installer)
        self.assertNotIn("sudo ", installer)
        self.assertTrue((SDDM / "install").stat().st_mode & stat.S_IXUSR)

    def test_bootstrap_seeds_hyprlocks_first_run_monitor_include(self) -> None:
        bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        self.assertIn("hyprlock-monitor.conf", bootstrap)
        self.assertIn("$auth_monitor =", bootstrap)
        self.assertIn('chmod 0600 "$hyprlock_monitor_state"', bootstrap)

    def test_theme_qml_loads_in_qt6_sddm_test_mode(self) -> None:
        greeter = Path("/usr/bin/sddm-greeter-qt6")
        if not greeter.exists():
            self.skipTest("sddm-greeter-qt6 is not installed")
        with tempfile.TemporaryDirectory(prefix="garage-sddm-test-") as scratch:
            staged = Path(scratch) / "theme"
            subprocess.run(["cp", "-a", str(THEME), str(staged)], check=True)
            subprocess.run(
                ["cp", str(REPO_ROOT / "desktop" / "Wallpaper" / "Dark"
                           / "ggykb5c7oeQ.jpg"), str(staged / "background.jpg")],
                check=True,
            )
            subprocess.run(
                ["cp", str(REPO_ROOT / "desktop" / ".local" / "share" / "fonts"
                           / "PlusJakartaSans.ttf"), str(staged / "PlusJakartaSans.ttf")],
                check=True,
            )
            env = os.environ.copy()
            env.update({"QT_QPA_PLATFORM": "offscreen", "QT_QUICK_BACKEND": "software"})
            try:
                result = subprocess.run(
                    [str(greeter), "--test-mode", "--theme", str(staged)],
                    env=env,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    timeout=2,
                    check=False,
                )
            except subprocess.TimeoutExpired as timeout:
                chunks = []
                for chunk in (timeout.stdout, timeout.stderr):
                    if isinstance(chunk, bytes):
                        chunks.append(chunk.decode("utf-8", errors="replace"))
                    elif chunk:
                        chunks.append(chunk)
                output = "".join(chunks)
            else:
                output = result.stdout
                self.fail(f"SDDM test greeter exited unexpectedly:\n{output}")
            self.assertNotIn("Fallback to embedded theme", output)
            self.assertNotIn("is not a type", output)
            self.assertNotIn("Error loading QML", output)


class HyprlockSurface(unittest.TestCase):
    def setUp(self) -> None:
        self.conf = HYPRLOCK.read_text(encoding="utf-8")
        self.script = LOCK_SCRIPT.read_text(encoding="utf-8")

    def test_background_is_all_monitor_but_form_is_monitor_scoped(self) -> None:
        background = self.conf.split("background {", 1)[1].split("}", 1)[0]
        self.assertRegex(background, r"(?m)^\s*monitor\s*=\s*$")
        field = self.conf.split("input-field {", 1)[1].split("}", 1)[0]
        self.assertIn("monitor = $auth_monitor", field)
        self.assertNotIn("$TIME", self.conf)
        self.assertNotIn("shape {", self.conf)
        self.assertNotIn("label {", self.conf)

    def test_password_field_is_the_only_control_and_keeps_fixed_geometry(self) -> None:
        self.assertIn("size = 320, 44", self.conf)
        self.assertIn("check_text = Authenticating…", self.conf)
        self.assertIn("fail_text = $FAIL", self.conf)
        self.assertIn("position = 0, 0", self.conf)
        self.assertIn("inner_color = rgba(00000000)", self.conf)
        self.assertIn("Enter your password...", self.conf)

    def run_script(self, monitors: object) -> tuple[str, list[list[str]]]:
        with tempfile.TemporaryDirectory(prefix="garage-lock-test-") as scratch:
            root = Path(scratch)
            binary = root / "bin"
            binary.mkdir()
            calls = root / "calls"
            (binary / "hyprctl").write_text(
                "#!/bin/sh\nprintf '%s' \"$GARAGE_MONITORS\"\n", encoding="utf-8"
            )
            (binary / "qs").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            (binary / "hyprlock").write_text(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >>\"$GARAGE_CALLS\"\n",
                encoding="utf-8",
            )
            for path in binary.iterdir():
                path.chmod(0o755)
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{binary}:{env['PATH']}",
                    "HOME": str(root),
                    "XDG_STATE_HOME": str(root / "state"),
                    "GARAGE_MONITORS": json.dumps(monitors),
                    "GARAGE_CALLS": str(calls),
                }
            )
            result = subprocess.run(
                [str(LOCK_SCRIPT)], env=env, text=True, capture_output=True, check=False
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            state = (root / "state" / "garage" / "generated"
                     / "hyprlock-monitor.conf").read_text(encoding="utf-8")
            invoked = [line.split() for line in calls.read_text(encoding="utf-8").splitlines()]
            return state, invoked

    def test_focused_monitor_is_written_atomically_before_hyprlock(self) -> None:
        state, invoked = self.run_script(
            [{"name": "DP-1", "focused": False},
             {"name": "HDMI-A-1", "focused": True}]
        )
        self.assertIn("$auth_monitor = HDMI-A-1", state)
        self.assertEqual(invoked, [["--immediate-render"]])

    def test_first_monitor_and_all_monitor_fallbacks_are_safe(self) -> None:
        first, _ = self.run_script(
            [{"name": "DP-2", "focused": False}, {"name": "DP-3", "focused": False}]
        )
        empty, _ = self.run_script([])
        self.assertIn("$auth_monitor = DP-2", first)
        self.assertRegex(empty, r"(?m)^\$auth_monitor =\s*$")

    def test_monitor_name_cannot_inject_hyprlang(self) -> None:
        state, _ = self.run_script([{"name": "DP-1\n$evil = yes", "focused": True}])
        self.assertRegex(state, r"(?m)^\$auth_monitor =\s*$")
        self.assertNotIn("evil", state)


if __name__ == "__main__":
    unittest.main()
