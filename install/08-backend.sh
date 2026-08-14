# shellcheck shell=bash
# INSTALL.md row 8: build, install, and link the five Rust backend
# binaries.

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

# Backend commands always link to the installed binaries. Stow no longer owns
# these names because desktop/.local/bin does not ship them.
for garage_binary in garage garage-metrics garage-file-index garage-ai-usage garage-waybar-module; do
    run ln -sfn "$garage_bin_dir/$garage_binary" "$HOME/.local/bin/$garage_binary"
done

# This is the ONLY migration call site. It runs the registry of the binary just
# built; putting it in the update step would silently run the OLD binary's
# registry instead. Both entry points -- bare bootstrap and `garage update` via
# its bootstrap step -- reach this point, and a migration may assume bootstrap
# has just converged.
run "$garage_bin_dir/garage" migrate
record "built the Rust backend"
