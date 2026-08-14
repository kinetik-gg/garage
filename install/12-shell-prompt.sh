# shellcheck shell=bash
# INSTALL.md row 12: install the pinned Pure Fish prompt when needed.

# ---------------------------------------------------------------------------
# Shell prompt
# ---------------------------------------------------------------------------

step "Setting up the shell prompt"

# Pinned: an unpinned prompt plugin is a third party with write access to every
# future shell start. `fisher install owner/repo@ref` checks out that ref.
pure_pin="pure-fish/pure@v4.18.0"
if ((dry_run)) || fish -c 'type -q fisher'; then
    # fish_plugins is fisher's own manifest and is what it reconciles against.
    # `fisher list` is not used here: it reads a fish universal variable, which
    # is per-machine state that does not exist on a first run and can go missing
    # on a machine where the prompt is in fact installed.
    if grep -qF 'pure-fish/pure' "$HOME/.config/fish/fish_plugins" 2>/dev/null; then
        info "the Pure prompt is already installed."
    else
        run fish -c "fisher install $pure_pin"
        record "installed the Pure prompt ($pure_pin)"
    fi
fi
