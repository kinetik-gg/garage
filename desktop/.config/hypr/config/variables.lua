-- Hyprland default apps

TERMINAL     = os.getenv("HYPR_TERMINAL") or "kitty"
FILE_MANAGER = os.getenv("HYPR_FILE_MANAGER") or ((os.getenv("HOME") or "") .. "/.local/bin/garage-open-files")
BROWSER      = os.getenv("BROWSER") or "google-chrome-stable"
EDITOR       = os.getenv("HYPR_EDITOR") or "gnome-text-editor --new-window"
CALCULATOR   = os.getenv("HYPR_CALCULATOR") or "gnome-calculator"

-- Monitors
-- No connector is named here, and none is named in config.monitors either.
-- Which outputs a machine has is a fact about that machine, not about this
-- configuration, so it is discovered rather than shipped: the Displays pane
-- records it in ~/.config/garage/displays.toml, `garage render` turns that into
-- the generated displays.lua, and that fragment sets PRIMARY_MONITOR along with
-- every named monitor rule. hyprland.lua loads it before config.windowrules and
-- config.workspaces, which are the only two readers of this name.
--
-- Left nil until then, on purpose. `monitor` is an optional field on both a
-- window rule and a workspace rule, so nil drops it from the table and the rule
-- carries no display constraint at all -- which is the only honest answer before
-- anything knows what the displays are. A guessed connector name would instead
-- match nothing on most machines, silently, and an empty string would still be
-- asserting that a display is called something.
PRIMARY_MONITOR = os.getenv("HYPR_PRIMARY_MONITOR")

-- Workspaces
-- Which display owns which workspace IDs, and whether they are pinned at all.
-- config.workspaces turns this into rules and config.binds into the number
-- keys, so the two cannot drift apart.
--
-- This is the hardware-neutral baseline for a session that has never rendered:
-- ten workspaces, one per number key, pinned to nothing. Shared mode writes no
-- numeric workspace rule at all, so a workspace opens on whichever display asks
-- for it and stays there -- Hyprland with no rules, its own default -- and that
-- is the only plan that can be stated before the displays are known. Pinning to
-- a guessed connector instead is what the DP-named baseline used to do, and on
-- any other machine it created a screenful of persistent workspaces belonging to
-- outputs that did not exist and left every number key addressing a display the
-- plan had never heard of. `monitor = ""` is how a group spells "no display",
-- the same spelling garage's own shared mode emits; ten is the whole of the
-- number row, so no key is dead before the first render.
--
-- The real plan is generated from the displays actually attached and loaded
-- below -- here, and not from hyprland.lua's override block, because
-- config.binds is required before that block runs and needs the same plan.
-- There each display owns a block of ten ids and keeps `count` of them, so the
-- blocks do not move when a count changes. Ten is also the most workspaces a
-- display can be given, so a block is never too small.
WORKSPACE_PLAN = {
    mode = "shared",
    groups = {
        { monitor = "", first = 1, count = 10 },
    },
}

-- Silent on failure, unlike hyprland.lua's loader, which reports a bad fragment
-- through hyprctl configerrors. Nothing above this line has run yet, so an
-- error here would abort config.variables and take the binds with it -- no
-- terminal, no launcher, no way to switch workspace. The baseline above is
-- always a working desktop, so falling back to it beats surfacing the fault.
-- The fragment is syntax-checked with luac before it is ever installed.
local generated_plan = (os.getenv("HOME") or "")
    .. "/.local/state/garage/generated/workspaces.lua"
local plan_file = io.open(generated_plan, "r")
if plan_file ~= nil then
    plan_file:close()
    pcall(dofile, generated_plan)
end
