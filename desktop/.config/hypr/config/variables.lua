-- Hyprland default apps

TERMINAL     = os.getenv("HYPR_TERMINAL") or "kitty"
FILE_MANAGER = os.getenv("HYPR_FILE_MANAGER") or "nautilus --new-window"
BROWSER      = os.getenv("BROWSER") or "google-chrome-stable"
EDITOR       = os.getenv("HYPR_EDITOR") or "gnome-text-editor --new-window"
CALCULATOR   = os.getenv("HYPR_CALCULATOR") or "gnome-calculator"

-- Monitors
MONITOR1 = os.getenv("HYPR_MONITOR1") or "DP-1"
MONITOR2 = os.getenv("HYPR_MONITOR2") or "DP-2"
MONITOR3 = os.getenv("HYPR_MONITOR3") or "DP-3"
PRIMARY_MONITOR = os.getenv("HYPR_PRIMARY_MONITOR") or MONITOR1

-- Workspaces
-- Which display owns which workspace IDs, and whether they are pinned at all.
-- config.workspaces turns this into rules and config.binds into the number
-- keys, so the two cannot drift apart.
--
-- This is the portable baseline for a session that has never run System
-- Preferences. The real plan is generated from the displays actually attached
-- and loaded below -- here, and not from hyprland.lua's override block, because
-- config.binds is required before that block runs and needs the same plan.
-- Each display owns a block of ten ids and keeps `count` of them, so the blocks
-- do not move when a count changes. Ten is the whole of the number row, which is
-- also the most workspaces a display can be given, so a block is never too small.
WORKSPACE_PLAN = {
    mode = "per-display",
    groups = {
        { monitor = MONITOR1, first = 1,  count = 8 },
        { monitor = MONITOR2, first = 11, count = 4 },
        { monitor = MONITOR3, first = 21, count = 4 },
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
