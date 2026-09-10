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
# A tag or commit can be selected explicitly, including from a detached checkout:
./iso/build offline '<git-ref>'
```

Finished images are written to `out/iso/`. Build work and package/toolchain
caches live under `.cache/iso/` and are reused by later builds. Set
`GARAGE_ISO_PACMAN_CACHE`, `GARAGE_ISO_OFFLINE_PACKAGE_CACHE`, or
`GARAGE_ISO_OFFLINE_TOOLCHAIN_CACHE` to reuse caches elsewhere.

The build resolves the selected ref (default `HEAD`) once. Its container reads
an archive of that commit, including the profile, scripts, package manifest, and
Cargo lockfile; its embedded Git bundle contains the same commit on an installable
`main` branch. Staged, unstaged, untracked, and ignored checkout files are excluded.
Commit the revision you intend to test before building it. Temporary source
snapshots are removed after the build; downloaded package/toolchain caches remain.

This pins Garage's source inputs, not all upstream inputs: the container image,
Arch repositories, ArchISO, and stable Rust toolchain still move. Release naming,
upstream pinning/provenance, and publication qualification remain separate work.

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

After a failed or interrupted run, remain in a real TTY login and resume with:

```sh
/usr/local/lib/garage/first-boot --retry
```

The explicit retry restores the offline environment when the payload is present
and permits bootstrap to continue on its partially installed target. Only one
bootstrap may run at a time. Once bootstrap succeeds, a checkpoint makes retries
finish cleanup without rerunning it. Offline cleanup restores the online pacman
configuration before removing its payload; failures remain retryable and never
print a completion message.

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
