#!/usr/bin/env bash
# shellcheck disable=SC2034

iso_variant="${GARAGE_ISO_VARIANT:-netinstall}"
iso_name="garage-$iso_variant"
iso_label="GARAGE_${iso_variant^^}_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="Kinetik <https://kinetik.wtf>"
if [[ $iso_variant == offline ]]; then
  iso_application="Garage offline installer"
else
  iso_application="Garage netinstall"
fi
iso_version="$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y.%m.%d)"
install_dir="arch"
buildmodes=('iso')
bootmodes=('bios.syslinux'
           'uefi.systemd-boot')
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"
airootfs_image_tool_options=('-comp' 'xz' '-Xbcj' 'x86' '-b' '1M' '-Xdict-size' '1M')
bootstrap_tarball_compression=('zstd' '-c' '-T0' '--auto-threads=logical' '--long' '-19')
file_permissions=(
  ["/etc/shadow"]="0:0:400"
  ["/root"]="0:0:750"
  ["/root/.automated_script.sh"]="0:0:755"
  ["/root/.gnupg"]="0:0:700"
  ["/usr/local/bin/choose-mirror"]="0:0:755"
  ["/usr/local/bin/garage-install"]="0:0:755"
  ["/usr/local/bin/Installation_guide"]="0:0:755"
  ["/usr/local/bin/livecd-sound"]="0:0:755"
  ["/opt/garage-installer/stage-first-boot"]="0:0:755"
  ["/opt/garage-installer/garage-first-boot"]="0:0:755"
  ["/opt/garage-installer/garage-first-boot-profile.sh"]="0:0:644"
)
