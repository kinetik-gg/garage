# shellcheck shell=bash
# Shared output, execution, summary, and link-resolution helpers.

# ---------------------------------------------------------------------------
# Output and execution helpers
# ---------------------------------------------------------------------------

step() { printf '\n==> %s\n' "$*"; }
info() { printf '    %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

# Every mutating command goes through run(). In --dry-run it is printed and not
# executed, which is what makes a dry run provably side-effect free: there is no
# second path that mutates anything.
run() {
    if ((dry_run)); then
        printf '    [dry-run] %s\n' "$*"
        return 0
    fi
    "$@"
}

# File bodies are written by heredoc/redirection, which run() cannot wrap.
write_file() {
    local path=$1
    if ((dry_run)); then
        printf '    [dry-run] write %s\n' "$path"
        cat >/dev/null
        return 0
    fi
    mkdir -p -- "$(dirname -- "$path")"
    cat >"$path"
}

summary=()
record() {
    summary+=("$1")
    info "$1"
}
# Where a symlink points after exactly one hop, as an absolute path, without
# resolving any further. Full resolution is wrong for this job: several tracked
# files are themselves symlinks (systemd .wants entries pointing at /usr/lib), so
# `readlink -m` on a perfectly good stow link lands outside the repository.
link_hop() {
    local target=$1 dest
    dest=$(readlink -- "$target")
    [[ $dest == /* ]] || dest="$(dirname -- "$target")/$dest"
    realpath -ms -- "$dest"
}

# True when a symlink is one of ours in *this* checkout. Checked lexically first,
# then through full resolution, so it holds whether or not the repository path
# itself contains symlinked components.
points_into_repo() {
    local target=$1
    [[ "$(link_hop "$target")" == "$repo_dir/desktop/"* ]] && return 0
    [[ "$(readlink -m -- "$target")" == "$repo_dir/desktop/"* ]] && return 0
    return 1
}

# Print the packages that satisfy GPU-specific runtime requirements for an
# `lspci -nn` inventory. Match numeric PCI class and vendor ids rather than
# translated device names, and install every detected hardware provider so a
# hybrid laptop can use either GPU. An unrecognised or virtual adapter gets
# Mesa's software Vulkan implementation: it is slower, but it is deterministic
# and avoids pacman choosing NVIDIA as the first provider of `vulkan-driver`.
gpu_packages_for() {
    local pci_inventory=${1-} gpu_inventory found_provider=0
    gpu_inventory=$(grep -E '\[(0300|0302|0380)\]' <<<"$pci_inventory" || true)

    if grep -qi '\[8086:' <<<"$gpu_inventory"; then
        printf '%s\n' vulkan-intel
        found_provider=1
    fi
    if grep -qi '\[1002:' <<<"$gpu_inventory"; then
        printf '%s\n' vulkan-radeon
        found_provider=1
    fi
    if grep -qi '\[10de:' <<<"$gpu_inventory"; then
        printf '%s\n' nvidia-open nvidia-utils egl-wayland libva-nvidia-driver
        found_provider=1
    fi

    if ((found_provider == 0)); then
        printf '%s\n' vulkan-swrast
    fi
}
