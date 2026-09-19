"""Copy Garage's revision-pinned installer payload into Archinstall's target."""

from pathlib import Path
import shutil

__archinstall__version__ = 4.4


class Plugin:
	"""Archinstall hook used by the Garage netinstall image."""

	def on_install(self, installer) -> None:
		"""Stage the live payload after the base system has been installed."""
		source = Path("/opt/garage-installer")
		target = installer.target / "opt/garage-installer"
		shutil.copytree(source, target, dirs_exist_ok=True)

		offline_source = Path("/opt/garage-offline")
		if offline_source.is_dir():
			offline_target = installer.target / "opt/garage-offline"
			shutil.copytree(offline_source, offline_target, dirs_exist_ok=True)
			shutil.copy2("/etc/pacman.conf", installer.target / "etc/pacman.conf")
