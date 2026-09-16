-- Execute the real entry point with Hyprland's deferred loader contract.
local config = assert(arg[1], "expected the hyprland.lua path")

local function run(loaded, present, fail)
    local queued, required = {}, {}
    local env = setmetatable({
        os = { getenv = function() return "/fixture" end },
        io = { open = function(path)
            if present and path:match("%.so$") then
                return { close = function() end }
            end
        end },
        require = function(name) required[name] = true end,
        hl = {
            plugin = { load = function(path)
                if fail then error("registration failed") end
                queued[#queued + 1] = path
                -- Hyprland returns nothing; it loads after this chunk exits.
            end },
            get_loaded_plugins = function() return loaded end,
        },
    }, { __index = _G })
    local chunk = assert(loadfile(config, "t", env))
    local ok, err = pcall(chunk)
    assert(required["config.binds"] and required["config.workspaces"],
        "plugin state must not stop the rest of the desktop config")
    return env, queued, ok, err
end

local env, queued, ok = run({}, true)
assert(ok and #queued == 2)
assert(not env.GLASS_AVAILABLE and not env.HYPREXPO_AVAILABLE,
    "a deferred or failed load must not enable unregistered plugin settings")

env, queued, ok = run({ { name = "kinetik-glass" } }, true)
assert(ok and #queued == 2)
assert(env.GLASS_AVAILABLE and not env.HYPREXPO_AVAILABLE)

env, queued, ok = run({ { name = "kinetik-glass" }, { name = "hyprexpo" } }, true)
assert(ok and #queued == 2 and env.GLASS_AVAILABLE and env.HYPREXPO_AVAILABLE)

env, queued, ok = run({}, false)
assert(ok and #queued == 0 and not env.GLASS_AVAILABLE and not env.HYPREXPO_AVAILABLE)

local err
env, queued, ok, err = run({}, true, true)
assert(not ok and tostring(err):match("registration failed"))
assert(not env.GLASS_AVAILABLE and not env.HYPREXPO_AVAILABLE)
print("plugin loading contracts passed")
