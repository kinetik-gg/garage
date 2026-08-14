# shellcheck shell=bash
# INSTALL.md row 10: classify the GPU and choose a safe initial
# window material without overriding existing preferences.

# ---------------------------------------------------------------------------
# The window material gate
#
# Garage ships glass_mode = "liquid", which is the right default for the machine
# it was developed on and the wrong one for a laptop. Kinetik Glass in any
# enabled mode captures the entire monitor framebuffer, downscales it and runs a
# multi-pass blur over it *per glass window per damaged frame* -- see
# Glass::Blur::refresh(), which blurs `CBox whole{{}, bufferSize}` and is gated
# only on the plugin being enabled at all. On a discrete GPU that is free. On
# integrated graphics, sharing system memory bandwidth with the CPU, it is a
# frame-rate collapse, and a first login that stutters reads as a broken
# desktop rather than as a setting.
#
# So the default is decided here, from the hardware, before anything renders.
# Nothing is written on a machine with a discrete GPU: the shipped default is
# already right there, and a file that says the same thing as layer 1 is only a
# copy that can drift.
#
# Placed after the stow step rather than next to the package phase because it
# writes into ~/.config/garage, which the shipped defaults reach as a stow
# symlink, and it must land before the first render -- which happens at the
# first graphical login, not here.
# ---------------------------------------------------------------------------

step "Choosing a window material this GPU can afford"

# Every GPU-class PCI device, in lspci's -nn form so the classifier can read
# both the vendor id and the marketing name. Matched on the class code
# (0300 VGA, 0302 3D, 0380 Display) rather than on lspci's English description,
# which is a translation away from changing.
gpu_devices() {
    command -v lspci >/dev/null || return 0
    lspci -nn 2>/dev/null | grep -E '\[03(00|02|80)\]' || true
}

# discrete | integrated | unknown, from the lines gpu_devices produced.
#
# "integrated" is the acting verdict, so it has to be reached from positive
# evidence: devices were found and none of them is a discrete GPU. An empty list
# is "unknown" and changes nothing -- and so is an lspci that is not installed.
gpu_verdict() {
    local list=$1 line discrete=0 found=0
    while IFS= read -r line; do
        [[ -n $line ]] || continue
        found=1
        case $line in
            *'[10de:'*)
                # NVIDIA has never shipped an x86 integrated GPU.
                discrete=1
                ;;
            *'[8086:'*)
                # Intel is integrated apart from its discrete Arc line, which
                # names itself in the device string.
                if [[ $line =~ (Arc|DG1|DG2|Alchemist|Battlemage) ]]; then
                    discrete=1
                fi
                ;;
            *'[1002:'* | *'[1022:'*)
                # AMD prints an APU as a *CPU* codename plus a bare
                # "[Radeon Graphics]" or "[Radeon Vega ... Graphics]" -- e.g.
                # "Granite Ridge [Radeon Graphics]" -- and a card as a GPU family
                # plus a model number: "Navi 31 [Radeon RX 7900 XT ...]". Keying
                # on the model number and the discrete family names means the
                # next APU codename is classified correctly without this file
                # being edited, which is the opposite of what an APU codename
                # list would do.
                if [[ $line =~ Radeon\ (RX|Pro|R[579])[[:space:]]*[0-9] ]]; then
                    discrete=1
                fi
                if [[ $line =~ (Navi|Ellesmere|Baffin|Polaris|Hawaii|Tonga|Fiji|Lexa|Oland|Bonaire|Pitcairn|Tahiti|Curacao) ]]; then
                    discrete=1
                fi
                ;;
            *)
                # An unrecognised vendor at a GPU class code is a virtual
                # adapter (virtio, QXL, VMware SVGA) or a server BMC (ASPEED,
                # Matrox). None of those can afford the effect either, and they
                # are exactly the machines a full-framebuffer blur punishes
                # hardest, so they count as evidence for the light default
                # rather than as no evidence at all.
                ;;
        esac
    done <<<"$list"
    if ((discrete)); then
        printf 'discrete\n'
    elif ((found)); then
        printf 'integrated\n'
    else
        printf 'unknown\n'
    fi
}

gpu_lines="$(gpu_devices)"
gpu_class="$(gpu_verdict "$gpu_lines")"
if [[ -n $gpu_lines ]]; then
    while IFS= read -r gpu_line; do
        info "found: ${gpu_line}"
    done <<<"$gpu_lines"
fi

preferences_file="$HOME/.config/garage/preferences.toml"

case $gpu_class in
    discrete)
        info "verdict: a discrete GPU is present."
        info "keeping the shipped default, Liquid Glass -- this machine can pay for it."
        ;;
    unknown)
        warn "no GPU-class PCI device could be identified (is lspci available?)."
        info "keeping the shipped default, Liquid Glass, since there is nothing to base a"
        info "  change on. If the desktop stutters, set the window material to Off in"
        info "  System Preferences > Appearance."
        ;;
    integrated)
        info "verdict: integrated graphics only."
        info "Liquid Glass blurs the whole monitor framebuffer for every glass window on"
        info "  every damaged frame, which an integrated GPU cannot keep up with. Frosted"
        info "  is not the cheaper option it looks like -- it flattens the bevel but still"
        info "  captures and blurs the full framebuffer -- so the default here is Off,"
        info "  which skips the plugin's render path entirely."
        if [[ -e $preferences_file || -L $preferences_file ]]; then
            info "your ~/.config/garage/preferences.toml already exists, so your settings are left"
            info "  alone. Set Appearance > Material to Off yourself if the desktop stutters."
        else
            # ---------------------------------------------------------------
            # ONE-WRITER VIOLATION, deliberate, narrow, and measured rather
            # than assumed.
            #
            # `garage` owns preferences.toml and nothing else should write it.
            # This does. The alternative was tried:
            #
            #   `garage set appearance.glass_mode '"off"'` writes the file and
            #   then applies the change, and applying reaches the compositor
            #   through `hyprctl`. From the TTY this bootstrap runs on there is
            #   no compositor, so it exits 1 with a JSON error -- survivable, the
            #   write lands first, and since the deltas-only work `set` writes
            #   only departures, so the old fossilization objection is gone.
            #   What remains is the exit-1-on-a-TTY wart and the apply noise.
            #
            # So: write the two keys directly and leave every other default
            # coming from layer 1, which is what the layering is for.
            #
            # TODO: once `garage` grows a non-applying write path (set --no-apply
            # or similar), this block becomes one call to it.
            # ---------------------------------------------------------------

            # Single-sourced from the tracked defaults rather than hardcoded, so
            # a schema bump cannot leave this writing a stamp that means
            # something else. An unreadable stamp is not fatal: a file without a
            # [schema] section reads as version 1, and garage's migrations for
            # 2, 3 and 4 only touch keys this file does not carry, then stamp it.
            schema_stamp="$(awk -F'[= ]+' '/^preferences_version/ { print $2; exit }' \
                "$repo_dir/desktop/.config/garage/preferences.defaults.toml" 2>/dev/null || true)"
            schema_block=""
            if [[ $schema_stamp =~ ^[0-9]+$ ]]; then
                schema_block="[schema]
preferences_version = ${schema_stamp}

"
            else
                warn "could not read preferences_version from the shipped defaults;"
                warn "  writing preferences.toml unstamped and letting garage migrate it."
            fi

            write_file "$preferences_file" <<PREFERENCES
# Written once by Garage's bootstrap, before the first login, because this
# machine reported integrated graphics only:
#
$(printf '#   %s\n' "${gpu_lines:-none}")
#
# Liquid Glass makes every glass window capture and blur the whole monitor
# framebuffer on every damaged frame. A discrete GPU can pay for that; shared
# memory bandwidth cannot. Off is the only mode that skips the work -- Frosted
# flattens the bevel but still captures and blurs the full framebuffer.
#
# This is a default, not a decision. Change it in System Preferences >
# Appearance, or:
#
#     garage set appearance.glass_mode '"liquid"'
#
# and nothing will overwrite you: bootstrap only writes this file when it does
# not already exist. Every other preference deliberately stays absent here so
# that it keeps coming from the shipped defaults.

${schema_block}[appearance]
glass_mode = "off"
PREFERENCES
            record "set the window material to Off for integrated graphics"
        fi
        ;;
esac
