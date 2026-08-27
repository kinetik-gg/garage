#!/usr/bin/env bash

set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

# shellcheck source=../install/lib.sh
source "$repo_dir/install/lib.sh"

grep -Eq '^qt6-5compat[[:space:]]+critical([[:space:]]|$)' \
    "$repo_dir/system/manifest/packages.list"
grep -Fqx 'QtVersion=6' "$repo_dir/system/sddm/garage/metadata.desktop"

assert_packages() {
    local name=$1 inventory=$2 expected=$3 actual
    actual=$(gpu_packages_for "$inventory" | paste -sd ' ' -)
    if [[ $actual != "$expected" ]]; then
        printf 'GPU package fixture %s failed:\n  expected: %s\n  actual:   %s\n' \
            "$name" "$expected" "$actual" >&2
        exit 1
    fi
}

assert_packages intel \
    '00:02.0 VGA compatible controller [0300]: Intel Corporation UHD Graphics [8086:9bc4]' \
    'vulkan-intel'

assert_packages amd \
    '05:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. [AMD/ATI] [1002:164e]' \
    'vulkan-radeon'

assert_packages nvidia \
    '01:00.0 3D controller [0302]: NVIDIA Corporation AD104M [GeForce RTX 4080 Max-Q / Mobile] [10de:27e0]' \
    'nvidia-open nvidia-utils egl-wayland libva-nvidia-driver'

assert_packages intel_nvidia_hybrid \
    $'00:02.0 VGA compatible controller [0300]: Intel Corporation Graphics [8086:46a6]\n01:00.0 3D controller [0302]: NVIDIA Corporation GPU [10de:25a0]' \
    'vulkan-intel nvidia-open nvidia-utils egl-wayland libva-nvidia-driver'

assert_packages amd_nvidia_hybrid \
    $'05:00.0 Display controller [0380]: Advanced Micro Devices, Inc. [AMD/ATI] GPU [1002:164e]\n01:00.0 VGA compatible controller [0300]: NVIDIA Corporation GPU [10de:2684]' \
    'vulkan-radeon nvidia-open nvidia-utils egl-wayland libva-nvidia-driver'

assert_packages virtio \
    '00:02.0 VGA compatible controller [0300]: Red Hat, Inc. Virtio 1.0 GPU [1af4:1050]' \
    'vulkan-swrast'

assert_packages unknown \
    '00:0f.0 VGA compatible controller [0300]: VMware SVGA II Adapter [15ad:0405]' \
    'vulkan-swrast'

assert_packages no_gpu '' 'vulkan-swrast'

# A vendor id on a non-GPU PCI function must not select that vendor's driver.
assert_packages nvidia_audio_only \
    '01:00.1 Audio device [0403]: NVIDIA Corporation Device [10de:22ba]' \
    'vulkan-swrast'

printf 'Install contracts cover the Qt runtimes and Intel, AMD, NVIDIA, hybrid, and fallback GPUs.\n'
