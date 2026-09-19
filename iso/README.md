# Garage ISO

This directory owns Garage's two bootable installer images:

- `netinstall` carries the Garage source revision used for the build, then
  downloads Arch and Garage packages during installation.
- `offline` additionally carries a signed local Arch package repository, the
  Rust toolchain and Cargo registry needed by Garage, and the two pinned source
  inputs used by bootstrap. Installation does not require a network connection.

## Build locally

The host needs Docker. `archiso` and its build dependencies run inside the
container, so the host does not need to be Arch Linux.

```sh
./iso/build netinstall
./iso/build offline
```

Finished images are written to `out/iso/`. Build work and package/toolchain
caches live under `.cache/iso/` and are reused by later builds. Set
`GARAGE_ISO_PACMAN_CACHE`, `GARAGE_ISO_OFFLINE_PACKAGE_CACHE`, or
`GARAGE_ISO_OFFLINE_TOOLCHAIN_CACHE` to reuse caches elsewhere.

The build embeds committed `HEAD`, not the working tree. This keeps local
changes and credentials out of an image and makes the source revision
traceable. Build or commit the revision you intend to test before distributing
the ISO.

Run the boot smoke against the newest local image with:

```sh
./iso/test-boot
```

The smoke boots a Garage image with QEMU/KVM and waits for the Garage
installer banner on a serial console. It needs `qemu-system-x86_64`, `bsdtar`,
`xorriso`, and working KVM access. It does not partition a disk or prove a
complete installation. When passed an offline image, it removes the VM network
device as part of the smoke.

## Installation flow

1. The live image opens the Garage installer and hands disk, locale, user, and
   bootloader choices to Archinstall.
2. Garage's Archinstall plugin copies the embedded installer payload into the
   new system.
3. A post-install command requires one administrator account, clones the
   embedded Garage Git bundle into that account's home, and schedules the
   Garage bootstrap for its first TTY login.
4. The user reboots, logs in normally, and Garage's existing `bootstrap.sh`
   runs in that real PAM/systemd user session. It asks for the user's sudo
   password in the normal way.
5. Whether bootstrap succeeds or fails, the automatic handoff is disabled. A
   failed run leaves a retry command on screen instead of creating a login
   loop.

The real-login handoff is intentional. Running `bootstrap.sh` in Archinstall's
chroot would not provide the systemd user manager that Garage validates before
making changes.

## Offline boundary

The offline image freezes the current Arch repository closure at build time and
records its package versions inside the image. After Garage finishes, the
installer restores the normal Arch repository configuration and removes its
temporary offline payload. Applications that fetch content at runtime still
need a connection later; for example, `spotify-launcher` cannot fetch Spotify
until the machine is online.
