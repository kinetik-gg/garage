-- Portable Hyprland configuration shared by CachyOS and vanilla Arch.

local home = os.getenv("HOME") or ""

local function load_plugin(name)
    local path = "/usr/lib/kinetik/plugins/" .. name .. ".so"
    local file = io.open(path, "r")
    if file == nil then
        return false
    end
    file:close()
    hl.plugin.load(path)
    return true
end

-- Plugins are optional during the first login. The rebuild script installs
-- ABI-pinned copies after Hyprland itself is available.
GLASS_AVAILABLE = load_plugin("kinetik-glass")
HYPREXPO_AVAILABLE = load_plugin("hyprexpo")

require("config.animations")
require("config.autostart")
require("config.decorations")
require("config.variables")
require("config.environment")
require("config.inputs")
require("config.binds")
require("config.misc")
require("config.monitors")

-- System Preferences writes machine-local overrides here. The tracked files
-- remain the portable baseline and continue to work when no override exists.
--
-- Guarded, unlike the requires above: these are the only files here that are
-- generated rather than tracked, and Hyprland's pre-apply syntax check covers
-- hyprland.lua but does not follow a dofile. An unguarded failure would abort
-- this chunk and take the window rules and workspace assignments with it, so a
-- bad fragment would look like a broken desktop rather than a bad fragment.
local fragment_errors = {}

local function load_override(path)
    local file = io.open(path, "r")
    if file == nil then
        return
    end
    file:close()
    local ok, err = pcall(dofile, path)
    if not ok then
        fragment_errors[#fragment_errors + 1] = tostring(err)
    end
end

load_override(home .. "/.local/state/garage/generated/displays.lua")
load_override(home .. "/.local/state/garage/generated/preferences.lua")

require("config.windowrules")
require("config.workspaces")

-- Raised last, so everything above is already applied. Hyprland collects a
-- config-time error into `hyprctl configerrors`, which puts it on screen in the
-- error bar instead of leaving the user with silently missing settings.
if #fragment_errors > 0 then
    error("garage generated fragment failed: " .. table.concat(fragment_errors, "; "), 0)
end
