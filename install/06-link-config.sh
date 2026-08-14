# shellcheck shell=bash
# INSTALL.md row 6: clear conflicts, link the tracked configuration,
# refresh fonts, seed Hyprlock, and build the Thunar GTK module.

# ---------------------------------------------------------------------------
# Link the tracked configuration into $HOME
#
# `stow` on its own aborts on the first conflict and leaves a half-linked home,
# and `stow -D` will not clean up absolute symlinks left behind by a checkout
# that has since moved. So: work out what stow is about to claim, move real
# files aside into a timestamped backup, delete stale links from other Garage
# checkouts by resolving them ourselves, and only then restow.
# ---------------------------------------------------------------------------

stow_ignore_patterns=()
while IFS= read -r pattern; do
    # Only the path-anchored entries matter here; the rest of the file repeats
    # stow's name-based defaults, which are handled below.
    [[ $pattern == '^/'* ]] && stow_ignore_patterns+=("$pattern")
done <"$repo_dir/desktop/.stow-local-ignore"

stow_ignores() {
    local rel="/$1" pattern
    for pattern in "${stow_ignore_patterns[@]}"; do
        [[ $rel =~ $pattern ]] && return 0
    done
    case ${1##*/} in
        .stow-local-ignore | .gitignore | *'~') return 0 ;;
    esac
    [[ $1 == .git/* || $1 == */.git/* ]] && return 0
    return 1
}

# Everything stow will place, as paths relative to $HOME. With --no-folding
# every leaf becomes its own symlink and every directory a real directory, so
# the conflict set is exactly the file list plus its ancestor directories.
managed_paths() {
    (cd "$repo_dir/desktop" && find . \( -type f -o -type l \) -printf '%P\n' | sort)
}

to_backup=()
to_unlink=()
declare -A ancestor_seen=()

# A symlink pointing at .../desktop/<the same relative path> is a stow link from
# a Garage checkout that has since moved. stow -D will not clean those up -- it
# only removes links it recognises as relative to the package it was given, and
# an old link is frequently absolute -- so resolve and delete them here instead
# of trusting stow with it.
foreign_garage_link() {
    local rel=$1
    [[ "$(link_hop "$HOME/$rel")" == */desktop/$rel ]] && return 0
    [[ ! -e "$HOME/$rel" ]] && return 0 # dangling: nothing of value to keep
    return 1
}

classify_ancestors() {
    local rel=$1 dir target
    dir=$(dirname -- "$rel")
    while [[ $dir != "." && $dir != "/" ]]; do
        if [[ -z ${ancestor_seen[$dir]-} ]]; then
            ancestor_seen[$dir]=1
            target="$HOME/$dir"
            if [[ -L $target ]]; then
                # A folded directory link -- from this checkout or another one --
                # blocks --no-folding from creating a real directory here.
                if points_into_repo "$target" || foreign_garage_link "$dir"; then
                    to_unlink+=("$dir")
                elif [[ ! -d $target ]]; then
                    to_backup+=("$dir")
                fi
            elif [[ -e $target && ! -d $target ]]; then
                to_backup+=("$dir")
            fi
        fi
        dir=$(dirname -- "$dir")
    done
}

step "Scanning \$HOME for anything in the way"
while IFS= read -r rel; do
    stow_ignores "$rel" && continue
    classify_ancestors "$rel"
    target="$HOME/$rel"
    if [[ -L $target ]]; then
        points_into_repo "$target" && continue # already ours
        if foreign_garage_link "$rel"; then
            to_unlink+=("$rel")
        else
            to_backup+=("$rel")
        fi
    elif [[ -e $target ]]; then
        to_backup+=("$rel")
    fi
done < <(managed_paths)

if ((${#to_unlink[@]})); then
    info "removing ${#to_unlink[@]} stale link(s) from a previous or moved checkout"
    for rel in "${to_unlink[@]}"; do
        info "  stale link: ~/$rel -> $(readlink -- "$HOME/$rel" 2>/dev/null || echo '?')"
        run rm -f -- "$HOME/$rel"
    done
    record "removed ${#to_unlink[@]} stale symlink(s)"
fi

if ((${#to_backup[@]})); then
    backup_root="$HOME/.garage-backup/$(date +%Y%m%d-%H%M%S)"
    info "moving ${#to_backup[@]} existing path(s) to $backup_root"
    for rel in "${to_backup[@]}"; do
        [[ -e "$HOME/$rel" || -L "$HOME/$rel" ]] || continue
        info "  backup: ~/$rel"
        run mkdir -p -- "$backup_root/$(dirname -- "$rel")"
        run mv -- "$HOME/$rel" "$backup_root/$rel"
    done
    record "backed up ${#to_backup[@]} pre-existing path(s) to $backup_root"
else
    info "nothing in the way."
fi

step "Linking the tracked configuration into \$HOME"
if ((dry_run)); then
    # --simulate is stow's own read-only mode. The per-link log is thousands of
    # lines, so count it and pass through only what is not a routine operation --
    # which is where a conflict report would appear.
    if ! command -v stow >/dev/null; then
        info "[dry-run] stow --dir=$repo_dir --target=$HOME --restow --no-folding desktop"
        warn "stow is not installed yet; skipping its link-conflict simulation."
    elif ! stow_output=$(stow --dir="$repo_dir" --target="$HOME" --restow \
        --no-folding --simulate --verbose=1 desktop 2>&1); then
        printf '%s\n' "$stow_output" | sed 's/^/    /' >&2
        if ((${#to_backup[@]} + ${#to_unlink[@]})); then
            # The simulate runs against the unmodified $HOME: the backups and
            # stale-link removals above have not actually happened in a dry run,
            # so stow still sees those paths in the way.
            info "[dry-run] conflicts at path(s) listed in the backup plan above are expected: a real run moves them before stow."
        else
            warn "stow --simulate reported a problem; see above."
        fi
    else
        operations=$(printf '%s\n' "$stow_output" | grep -c '^\(LINK\|UNLINK\|MKDIR\|RMDIR\):' || true)
        info "[dry-run] stow would perform $operations link operations."
        printf '%s\n' "$stow_output" | grep -v '^\(LINK\|UNLINK\|MKDIR\|RMDIR\):' |
            sed 's/^/    [dry-run] /' || true
    fi
else
    if ! stow_output=$(stow --dir="$repo_dir" --target="$HOME" --restow --no-folding desktop 2>&1); then
        printf '%s\n' "$stow_output" >&2
        cat >&2 <<'STOW'

stow could not link the configuration and this bootstrap has stopped rather
than leave your home half-installed. Each line above names a path that already
exists and is not a Garage link. Move those aside (or delete them) and re-run
./bootstrap.sh -- anything Garage itself knew about has already been moved to
~/.garage-backup/.

STOW
        exit 1
    fi
    [[ -n $stow_output ]] && printf '%s\n' "$stow_output"
fi
record "linked the tracked configuration with stow --no-folding"

# Refreshes the fontconfig cache so the bundled fonts -- Phosphor, Plus
# Jakarta Sans, and Geist Mono, just linked into ~/.local/share/fonts by
# stow -- are found at first login.
run fc-cache

# Hyprlock parses every source before garage-lock-session can generate its
# primary-monitor include. Seed base-scale geometry so a direct first-run
# invocation also has a complete configuration.
hyprlock_monitor_state="${XDG_STATE_HOME:-$HOME/.local/state}/garage/generated/hyprlock-monitor.conf"
if [[ ! -e $hyprlock_monitor_state && ! -L $hyprlock_monitor_state ]]; then
    write_file "$hyprlock_monitor_state" <<'HYPRLOCK_MONITOR'
# Generated by Garage. garage-lock-session replaces this before each lock.
$auth_monitor =
$auth_width = 320
$auth_height = 44
HYPRLOCK_MONITOR
    run chmod 0600 "$hyprlock_monitor_state"
    record "seeded Hyprlock's all-monitor fallback"
fi

# Thunar's shortcuts headers and rows share one GTK3 CSS node, Tree mode exposes
# a separate pane/view pair, and its CSD toolbar has no structural relationship
# with either resizable side pane. Compile the narrowly scoped module that
# supplies those missing semantics, plus Finder-like ruled rows, against the GTK
# version installed above. It returns immediately in every process but Thunar.
thunar_module_dir="${XDG_DATA_HOME:-$HOME/.local/share}/garage/gtk-modules"
run mkdir -p -- "$thunar_module_dir"
if ((dry_run)); then
    info "[dry-run] compile Garage's Thunar GTK module"
else
    read -r -a thunar_module_cflags <<<"$(pkg-config --cflags gtk+-3.0)"
    read -r -a thunar_module_libs <<<"$(pkg-config --libs gtk+-3.0)"
    run cc -O2 -shared -fPIC -Wall -Wextra -Werror \
        "${thunar_module_cflags[@]}" \
        -o "$thunar_module_dir/garage-thunar.so" \
        "$repo_dir/system/gtk-modules/garage-thunar.c" \
        "${thunar_module_libs[@]}"
fi
record "built Garage's Thunar-only GTK integration"
