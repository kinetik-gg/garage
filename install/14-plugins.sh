# shellcheck shell=bash
# INSTALL.md row 14: deploy optional Hyprland plugins when their
# source checkout is available.

step "Deploying the optional Hyprland plugins"
glass_repo=""
for candidate in "$HOME/repositories/glass" "$HOME/repositories/hyprliquid"; do
    [[ -d "$candidate/.git" ]] && {
        glass_repo=$candidate
        break
    }
done

if [[ ${GARAGE_SKIP_PLUGIN_DEPLOY:-0} == 1 ]]; then
    # `garage update` sets this. It makes the plugin decision itself -- comparing
    # the running ABI against what is deployed, and rebuilding only when they
    # disagree or a pin moved in the pull -- because a deploy needs sudo and
    # rebuilds nothing useful when the ABI has not moved. Deploying here as well
    # would do it twice on every update. An install never sets it: a fresh
    # machine has nothing deployed, so the deploy always has work to do.
    info "skipped: GARAGE_SKIP_PLUGIN_DEPLOY=1 (the caller owns the plugin decision)."
    summary+=("left the plugin deploy to the caller")
elif [[ -z $glass_repo ]]; then
    warn "no Glass plugin source at ~/repositories/glass -- skipping the plugin build."
    warn "  Garage's repositories are not published yet, so this is expected."
    warn "  The desktop runs without plugins; hyprland.lua treats both as optional."
    warn "  Once you have the source, run: ~/.config/hypr/scripts/garage-rebuild-plugins"
    summary+=("skipped the optional plugins (no Glass source)")
elif ((dry_run)); then
    info "[dry-run] $HOME/.config/hypr/scripts/garage-rebuild-plugins (source: $glass_repo)"
elif ! "$HOME/.config/hypr/scripts/garage-rebuild-plugins"; then
    warn "the plugin build failed; Hyprland will still start without the optional plugins."
    summary+=("optional plugin build FAILED (non-fatal)")
else
    record "deployed the pinned Hyprland plugins from $glass_repo"
fi
