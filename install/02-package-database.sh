# shellcheck shell=bash
# INSTALL.md row 2: build the manifest package set, refresh pacman,
# and verify every package name against the configured repositories.

# ---------------------------------------------------------------------------
# Packages
# ---------------------------------------------------------------------------

# The package set is data, not an array here: system/manifest/packages.list. The
# Rust port reads the same file, `garage doctor` reads the `critical` flag out of
# it, and bootstrap consumes that same inventory -- none of which is possible
# while the only copy is a bash array.
#
# The three lines inside the loop are the whole format: strip a trailing `#`
# comment, split the rest into fields, skip what is left of a blank or
# comment-only line. Field 2 (`critical`) is not read here; pacman installs
# every line regardless.
packages=()
while IFS= read -r manifest_line; do
    manifest_line=${manifest_line%%#*}
    read -r package_name _ <<<"$manifest_line"
    [[ -n $package_name ]] || continue
    packages+=("$package_name")
done <"$repo_dir/system/manifest/packages.list"

# A truncated or unreadable manifest would otherwise turn into a successful run
# that installs nothing and leaves you at a TTY wondering why.
if ((${#packages[@]} == 0)); then
    echo "error: system/manifest/packages.list named no packages." >&2
    exit 1
fi

# The one part of the package set that is a fact about the machine rather than
# about Garage, so it stays logic here instead of becoming a flag in the file.
# pciutils is in the `base` group, so lspci is there on a minimal install and
# this check works before the package phase. It is named in packages.list
# anyway: two decisions now depend on it -- the NVIDIA driver set here and the
# window material gate further down -- and neither should quietly fall back to
# "cannot tell" because something removed it.
if command -v lspci >/dev/null && lspci | grep -qi nvidia; then
    packages+=(nvidia-open nvidia-utils egl-wayland libva-nvidia-driver)
fi

step "Refreshing the package database and upgrading the system"
# A full upgrade with no targets first, so the name check below reads a synced
# database and the install itself is never a partial upgrade.
run sudo pacman -Syu

step "Verifying every package name against the repositories"
# A renamed or dropped package used to abort the run halfway through. Report the
# whole list up front instead.
if pacman -Si pacman >/dev/null 2>&1; then
    missing=()
    for package in "${packages[@]}"; do
        pacman -Si "$package" >/dev/null 2>&1 || missing+=("$package")
    done
    if ((${#missing[@]})); then
        printf 'error: these packages are not in the configured repositories:\n' >&2
        for package in "${missing[@]}"; do
            printf '  - %s\n' "$package" >&2
        done
        printf '\nRun `sudo pacman -Syy`, check that the extra repository is enabled,\n' >&2
        printf 'then fix system/manifest/packages.list before re-running.\n' >&2
        exit 1
    fi
    info "all ${#packages[@]} package names resolve."
else
    warn "the package database is not synced yet; skipping the name check."
fi
