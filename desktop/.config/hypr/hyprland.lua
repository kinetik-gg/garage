-- Portable Hyprland configuration shared by CachyOS and vanilla Arch.

local home = os.getenv("HOME") or ""

-- Collects config-time failures so they can be raised together at the end of
-- this chunk (see the `error(...)` call near the bottom). Populated by both
-- load_plugin below and load_override further down.
local fragment_errors = {}

local function load_plugin(name)
    local path = "/usr/lib/kinetik/plugins/" .. name .. ".so"
    local file = io.open(path, "r")
    if file == nil then
        return false
    end
    file:close()

    -- This only queues a load for the end of config parsing. A successful
    -- pcall does not prove the plugin loaded: an ABI mismatch is detected
    -- later, outside this call. Hyprland reloads the config after successful
    -- loading, so only enable plugin consumers once the loaded list agrees.
    local ok, err = pcall(hl.plugin.load, path)
    if not ok then
        fragment_errors[#fragment_errors + 1] = name
            .. " failed to load (stale ABI after a Hyprland update?): "
            .. tostring(err)
            .. " — run garage-rebuild-plugins"
        return false
    end
    for _, plugin in ipairs(hl.get_loaded_plugins()) do
        if plugin.name == name then
            return true
        end
    end
    return false
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
-- (fragment_errors itself is declared up top, next to load_plugin, since
-- both feed the same error(...) call below.)

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
-- error bar instead of leaving the user with silently missing settings. Covers
-- both load_plugin failures (stale-ABI plugin .so) and load_override failures
-- (bad generated fragment) collected into fragment_errors above.
if #fragment_errors > 0 then
    error("garage config issue(s) detected: " .. table.concat(fragment_errors, "; "), 0)
end
