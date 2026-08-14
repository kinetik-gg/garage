# shellcheck shell=bash
# INSTALL.md row 8: build, install, and link the five Rust backend
# binaries without changing the existing command-link policy.

step "Building and installing the Rust backend"
info "compiling the five Rust backend release binaries"
run cargo build --release --manifest-path "$repo_dir/backend/Cargo.toml"

garage_bin_dir="$HOME/.local/lib/garage/bin"
run mkdir -p -- "$garage_bin_dir"
garage_binaries=(
    garage
    garage-metrics
    garage-file-index
    garage-ai-usage
    garage-waybar-module
)
for garage_binary in "${garage_binaries[@]}"; do
    run install -m 755 \
        "$repo_dir/backend/target/release/$garage_binary" \
        "$garage_bin_dir/$garage_binary"
done

# Before the Python backend is deleted, stow-managed command links into this
# checkout keep owning their names. After deletion, an absent or non-repo link
# is refreshed to the installed Rust binary; the Rust-only Waybar module is
# always linked because Python and stow never own that name.
if ((dry_run)); then
    info "[dry-run] keep repo-owned Python command links; refresh every unclaimed command to $garage_bin_dir"
fi
for garage_binary in garage garage-metrics garage-file-index garage-ai-usage; do
    garage_command_path="$HOME/.local/bin/$garage_binary"
    if [[ -L $garage_command_path ]]; then
        garage_command_target=$(readlink -m -- "$garage_command_path")
        # A repo-owned link keeps its name only while its target still exists:
        # a dangling link into the checkout is a leftover from before the
        # Python backend was deleted, and preserving it would leave the command
        # broken until the next restow. Repoint it like any unclaimed name.
        if [[ -e $garage_command_target ]] &&
            [[ $garage_command_target == "$repo_dir" || $garage_command_target == "$repo_dir/"* ]]; then
            info "keeping $garage_command_path (owned by the stowed Python backend)"
            continue
        fi
    fi
    run ln -sfn "$garage_bin_dir/$garage_binary" "$garage_command_path"
done
run ln -sfn \
    "$garage_bin_dir/garage-waybar-module" \
    "$HOME/.local/bin/garage-waybar-module"
record "built the Rust backend"
