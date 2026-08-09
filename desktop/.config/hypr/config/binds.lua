local mainMod = "SUPER"
local launchPrefix = "uwsm app -- " -- if you are not using UWSM, make this empty (e.g. "")

local generated = (os.getenv("HOME") or "") .. "/.local/state/garage/generated/"
-- What System Preferences writes when a shortcut is changed or invented, and
-- where this file publishes the defaults for it to list. Both sit under
-- generated/ because ~/.config/hypr is a stow symlink farm into the dotfiles
-- repo, and nothing may write a tracked file at runtime.
local OVERRIDES_FILE = generated .. "keybinds.json"
local CATALOG_FILE = generated .. "keybinds.catalog"

-- The binds no override can move. Keyed by the normalised combination this file
-- gives them, which is also their id below. Hyprland's own emergency binds only
-- appear when the bind count reaches zero, which cannot happen here -- so if a
-- generated file ever asked for a set with no terminal and no launcher in it,
-- nothing else would catch it. These two stay where they are instead.
local RESCUE = {
    ["super+return"] = true, -- Open a terminal
    ["super+space"] = true,  -- Open the application launcher
}

-- The id a bind is known by, in the Keyboard pane and in the overrides file:
-- its default combination, with the spacing and case that only ever mattered to
-- whoever was reading it removed. Deriving it from the combination rather than
-- naming each of the hundred-odd binds by hand keeps the two from drifting, and
-- it is the same normalisation hl.unbind matches on.
local function bind_id(keys)
    return (tostring(keys):gsub("%s+", ""):lower())
end

-- A JSON reader, not another dofile.
--
-- Everything in the overrides file is text the user typed into a settings pane
-- -- a shell command, the name they gave it -- and running it as Lua would make
-- the command field a place to write Lua rather than shell. A demonstrated
-- injection through a naive `bind("..." )` template is exactly why this exists:
-- the fields arrive as data and stay data. Nothing below can return anything
-- but strings and tables of strings, so there is no path from the file to code.
--
-- Deliberately strict: numbers, `true` and `null` have no place in this file, so
-- accepting them would only widen what the reader has to be right about.
local function decode_json(text)
    local at = 1

    local function skip()
        at = text:find("[^ \t\r\n]", at) or #text + 1
    end

    local ESCAPES = { ['"'] = '"', ["\\"] = "\\", ["/"] = "/", b = "\b",
                      f = "\f", n = "\n", r = "\r", t = "\t" }

    local function read_string()
        assert(text:sub(at, at) == '"', "expected a string")
        at = at + 1
        local parts = {}
        while true do
            local char = text:sub(at, at)
            assert(char ~= "", "unterminated string")
            at = at + 1
            if char == '"' then
                return table.concat(parts)
            elseif char ~= "\\" then
                parts[#parts + 1] = char
            else
                local code = text:sub(at, at)
                at = at + 1
                if code == "u" then
                    local hex = text:sub(at, at + 3)
                    assert(hex:match("^%x%x%x%x$"), "malformed \\u escape")
                    at = at + 4
                    parts[#parts + 1] = utf8.char(tonumber(hex, 16))
                else
                    parts[#parts + 1] = assert(ESCAPES[code], "unknown escape")
                end
            end
        end
    end

    local function read_value()
        skip()
        local opening = text:sub(at, at)
        if opening == '"' then
            return read_string()
        end
        assert(opening == "{" or opening == "[", "expected a string, object or array")
        local object = opening == "{"
        local closing = object and "}" or "]"
        local value, index = {}, 0
        at = at + 1
        skip()
        if text:sub(at, at) == closing then
            at = at + 1
            return value
        end
        while true do
            local key
            if object then
                skip()
                key = read_string()
                skip()
                assert(text:sub(at, at) == ":", "expected :")
                at = at + 1
            else
                index = index + 1
                key = index
            end
            value[key] = read_value()
            skip()
            local delimiter = text:sub(at, at)
            at = at + 1
            if delimiter == closing then
                return value
            end
            assert(delimiter == ",", "expected , or " .. closing)
        end
    end

    local document = read_value()
    skip()
    assert(at > #text, "trailing content")
    return document
end

-- Silent on every failure, the way config.variables reads its workspace plan:
-- this runs before a single bind is registered, so raising here would abort the
-- chunk and leave the desktop with no keys at all. The tracked defaults below
-- are always a working desktop, which beats surfacing the fault.
local function read_overrides()
    local file = io.open(OVERRIDES_FILE, "r")
    if file == nil then
        return {}, {}
    end
    local text = file:read("a")
    file:close()
    local ok, document = pcall(decode_json, text or "")
    if not ok or type(document) ~= "table" then
        return {}, {}
    end
    return type(document.overrides) == "table" and document.overrides or {},
           type(document.custom) == "table" and document.custom or {}
end

local overrides, custom_binds = read_overrides()

-- The default set, collected as it is declared and published at the end of the
-- file. A settings pane has no other way to learn it: hyprctl reports the
-- combination a bind ended up on, never the one this file wanted, and the two
-- differ precisely when there is something to show.
local catalog = {}
-- Which section of this file the binds below belong to, so the pane can group
-- them the way the file already does. Reassigned as the sections go past.
local group = "Windows"

-- `hyprctl binds -j` cannot describe this config on its own: hl.bind stores every
-- handler in the Lua registry, so all of them report dispatcher "__lua" with an
-- opaque registry index, and displayKey is never serialised either. `description`
-- is the one field that does survive into the JSON (it also flips has_description),
-- which makes it the only way a settings UI can ever list these shortcuts.
--
-- So every bind goes through this wrapper instead of hl.bind directly, and the
-- description sits in argument position 2 where it cannot be forgotten, rather
-- than buried in an optional trailing table. The real
-- signature underneath is hl.bind(keys, dispatcher_or_function, opts), and opts
-- still carries repeating / locked / non_consuming / release / ...; it stays
-- available as the fourth argument.
--
-- A missing description is not fatal on purpose. Aborting the chunk here would
-- leave the remaining binds unregistered -- i.e. no terminal and no launcher --
-- so an authoring mistake degrades to a visibly ugly entry in the settings list
-- instead of a lockout.
local function bind(keys, description, action, opts)
    local options = {}
    if opts ~= nil then
        for key, value in pairs(opts) do
            options[key] = value
        end
    end

    options.description = description
    if type(description) ~= "string" or description == "" then
        options.description = "(missing description: " .. tostring(keys) .. ")"
    end

    -- A bare string is shorthand for a shell command. Every hl.dsp.* constructor
    -- returns userdata, so there is no ambiguity between the two forms.
    if type(action) == "string" then
        action = hl.dsp.exec_cmd(action)
    end

    -- The default, not whatever it is about to be moved to: the pane needs both
    -- so it can offer "restore", and the id is the default by definition.
    local id = bind_id(keys)
    catalog[#catalog + 1] = table.concat(
        { id, group, keys, RESCUE[id] and "1" or "0", options.description }, "\t")

    -- Registered once, at either the default combination or the user's, so the
    -- merged set is the whole bind set. Adding the override beside the default
    -- would leave both live: addKeybind is a bare emplace_back and the dispatch
    -- path runs every match, so there is no last-one-wins to rely on.
    --
    -- A rescue bind never consults the overrides at all. That is what makes the
    -- guarantee structural rather than a check that could be got round: whatever
    -- the generated file says, SUPER+Return still opens a terminal.
    local replacement = not RESCUE[id] and overrides[id] or nil
    if type(replacement) == "string" and replacement ~= "" then
        keys = replacement
    end

    return hl.bind(keys, action, options)
end

---------------------------
---- WINDOW MANAGEMENT ----
---------------------------

-- Window manipulation
bind(mainMod .. " + W",             "Close the focused window",              hl.dsp.window.close())
bind(mainMod .. " + SHIFT + Space", "Toggle the focused window floating",    hl.dsp.window.float({ action = "toggle" }))
bind(mainMod .. " + D",             "Maximise the focused window",           hl.dsp.window.fullscreen({ mode = 1 }))
bind(mainMod .. " + F",             "Fullscreen the focused window",         hl.dsp.window.fullscreen())
bind(mainMod .. " + J",             "Toggle split direction for the next window", hl.dsp.layout("togglesplit"))
-- Pairs with SUPER+J: both change how the focused window occupies its tile.
bind(mainMod .. " + SHIFT + J",     "Toggle pseudotiling for the focused window", hl.dsp.window.pseudo())

-- Change focus
bind(mainMod .. " + Left",          "Focus the window to the left",  hl.dsp.focus({ direction = "left" }))
bind(mainMod .. " + Right",         "Focus the window to the right", hl.dsp.focus({ direction = "right" }))
bind(mainMod .. " + Up",            "Focus the window above",        hl.dsp.focus({ direction = "up" }))
bind(mainMod .. " + Down",          "Focus the window below",        hl.dsp.focus({ direction = "down" }))
bind(mainMod .. " + CONTROL + H",   "Focus the window to the left",  hl.dsp.focus({ direction = "left" }))
bind(mainMod .. " + CONTROL + L",   "Focus the window to the right", hl.dsp.focus({ direction = "right" }))
bind(mainMod .. " + CONTROL + K",   "Focus the window above",        hl.dsp.focus({ direction = "up" }))
bind(mainMod .. " + CONTROL + J",   "Focus the window below",        hl.dsp.focus({ direction = "down" }))
bind("ALT + Tab",                   "Cycle to the next window",      hl.dsp.window.cycle_next())
bind(mainMod .. " + Tab",           "Cycle to the next window",      hl.dsp.window.cycle_next())
if HYPREXPO_AVAILABLE then
    bind("CONTROL + Up", "Show all workspaces (overview)", function() hl.plugin.hyprexpo.expo("toggle") end)
end

-- Move active window around workspaces & monitors
bind(mainMod .. " + SHIFT + Up",                   "Move the window up",                     hl.dsp.window.move({ direction = "u" }))
bind(mainMod .. " + SHIFT + Right",                "Move the window right",                  hl.dsp.window.move({ direction = "r" }))
bind(mainMod .. " + SHIFT + Left",                 "Move the window left",                   hl.dsp.window.move({ direction = "l" }))
bind(mainMod .. " + SHIFT + Down",                 "Move the window down",                   hl.dsp.window.move({ direction = "d" }))
bind(mainMod .. " + CONTROL + SHIFT + H",          "Move the window left",                   hl.dsp.window.move({ direction = "l" }))
bind(mainMod .. " + CONTROL + SHIFT + L",          "Move the window right",                  hl.dsp.window.move({ direction = "r" }))
bind(mainMod .. " + CONTROL + SHIFT + K",          "Move the window up",                     hl.dsp.window.move({ direction = "u" }))
bind(mainMod .. " + CONTROL + SHIFT + J",          "Move the window down",                   hl.dsp.window.move({ direction = "d" }))
bind(mainMod .. " + SHIFT + mouse_up",             "Move the window to the previous display", hl.dsp.window.move({ monitor   = "-1" }))
bind(mainMod .. " + SHIFT + mouse_down",           "Move the window to the next display",     hl.dsp.window.move({ monitor   = "+1" }))
bind(mainMod .. " + CONTROL + SHIFT + Right",      "Move the window to the next workspace",     hl.dsp.window.move({ workspace = "m+1" }))
bind(mainMod .. " + CONTROL + SHIFT + Left",       "Move the window to the previous workspace", hl.dsp.window.move({ workspace = "m-1" }))
bind(mainMod .. " + CONTROL + SHIFT + mouse_up",   "Move the window to the previous workspace", hl.dsp.window.move({ workspace = "m-1" }))
bind(mainMod .. " + CONTROL + SHIFT + mouse_down", "Move the window to the next workspace",     hl.dsp.window.move({ workspace = "m+1" }))

-- Swap the focused window with its neighbour, which is not the same as moving it:
-- move re-parents the window into the tree, swap exchanges two tiles in place.
-- SUPER+SHIFT+arrows is already move here, so swap gets its own modifier family
-- rather than repurposing a combo the user already has in their fingers.
bind(mainMod .. " + ALT + Left",  "Swap the window with the one to its left",  hl.dsp.window.swap({ direction = "left" }))
bind(mainMod .. " + ALT + Right", "Swap the window with the one to its right", hl.dsp.window.swap({ direction = "right" }))
bind(mainMod .. " + ALT + Up",    "Swap the window with the one above it",     hl.dsp.window.swap({ direction = "up" }))
bind(mainMod .. " + ALT + Down",  "Swap the window with the one below it",     hl.dsp.window.swap({ direction = "down" }))

-- Move & Resize with mouse
bind(mainMod .. " + mouse:272", "Drag to move a window",   hl.dsp.window.drag())
bind(mainMod .. " + mouse:273", "Drag to resize a window", hl.dsp.window.resize())

-- Zoom
group = "Zoom"
local function zoomfunction(value)
    local zoomvalue = hl.get_config("cursor:zoom_factor")
    if (zoomvalue + value) > 3.0 then
        hl.config({ cursor = { zoom_factor = 3.0 } })
    elseif (zoomvalue + value) < 1.0 then
        hl.config({ cursor = { zoom_factor = 1.0 } })
    else
        hl.config({ cursor = { zoom_factor = zoomvalue + value } })
    end
end
bind(mainMod .. " + Minus", "Zoom the screen out", function() zoomfunction(-0.3) end, { repeating = true })
bind(mainMod .. " + Plus",  "Zoom the screen in",  function() zoomfunction(0.3) end,  { repeating = true })

--# Zoom with keypad
-- These are alive despite looking dead in `hyprctl binds -j`: a code:NN key is
-- parsed into sMkKeys, which is not serialised, so both report key "" and
-- keycode 0. KeybindManager dispatches them from that field. Do not "clean up".
bind(mainMod .. " + code:82", "Zoom the screen out (numpad 0)", function() zoomfunction(-0.3) end, { repeating = true })
bind(mainMod .. " + code:86", "Zoom the screen in (numpad +)",  function() zoomfunction(0.3) end,  { repeating = true })


------------------
---- LAUNCHER ----
------------------

group = "Applications"

-- The terminal chosen in System Preferences, when one has been chosen. Applied
-- over the variables.lua default rather than beside it so every bind below --
-- including the one that runs btop inside a terminal -- follows the setting.
-- Read at parse time, so the helper reloads the config after publishing it.
local function published(name)
    local handle = io.open(
        (os.getenv("HOME") or "") .. "/.local/state/garage/generated/" .. name, "r")
    if handle == nil then
        return nil
    end
    local value = handle:read("l")
    handle:close()
    if value == nil or value == "" then
        return nil
    end
    return value
end

TERMINAL = published("terminal") or TERMINAL
-- $BROWSER is xdg-open, which is correct for anything passing it a URL and does
-- nothing as a bare command. The bind wants the browser's own executable.
BROWSER = published("browser") or BROWSER

bind(mainMod .. " + Return",     "Open a terminal",            launchPrefix .. TERMINAL)
bind(mainMod .. " + E",          "Open the file manager",      launchPrefix .. FILE_MANAGER)
bind(mainMod .. " + T",          "Open the text editor",       launchPrefix .. EDITOR)
bind(mainMod .. " + SHIFT + C",  "Open the calculator",        launchPrefix .. CALCULATOR)
bind("XF86Calculator",           "Open the calculator",        launchPrefix .. CALCULATOR)
bind(mainMod .. " + B",          "Open the web browser",       launchPrefix .. BROWSER)
bind("CONTROL + SHIFT + Escape", "Open the system monitor",    launchPrefix .. TERMINAL .. " -e btop")
-- Through the wrapper, not the shell's ipc directly: it is what decides between
-- the built-in launcher and rofi/wofi, so this bind never has to change.
bind(mainMod .. " + Space",      "Open the application launcher", (os.getenv("HOME") or "") .. "/.local/bin/garage-launcher-toggle")
bind(mainMod .. " + L",          "Lock the screen",            "hyprlock")

---------------------------
---- HARDWARE CONTROLS ----
---------------------------

group = "Media & Hardware"

-- Audio
bind("XF86AudioRaiseVolume", "Raise the volume",          "swayosd-client --monitor $(hyprctl activeworkspace -j | jq -r .monitor) --output-volume +5 --max-volume 100", { locked = true, repeating = true })
bind("XF86AudioLowerVolume", "Lower the volume",          "swayosd-client --monitor $(hyprctl activeworkspace -j | jq -r .monitor) --output-volume -5",                  { locked = true, repeating = true })
bind("XF86AudioMute",        "Mute or unmute the output", "swayosd-client --monitor $(hyprctl activeworkspace -j | jq -r .monitor) --output-volume mute-toggle",         { locked = true })
bind("XF86AudioMicMute",     "Mute or unmute the microphone", "swayosd-client --monitor $(hyprctl activeworkspace -j | jq -r .monitor) --input-volume mute-toggle",      { locked = true })

-- Media
bind("XF86AudioPlay",  "Play or pause playback",  "playerctl play-pause", { locked = true })
bind("XF86AudioPause", "Play or pause playback",  "playerctl play-pause", { locked = true })
bind("XF86AudioNext",  "Skip to the next track",  "playerctl next",       { locked = true })
bind("XF86AudioPrev",  "Go to the previous track", "playerctl previous",  { locked = true })

-- Brightness
bind("XF86MonBrightnessUp",   "Increase display brightness", "swayosd-client --monitor $(hyprctl activeworkspace -j | jq -r .monitor) --brightness +5", { locked = true, repeating = true })
bind("XF86MonBrightnessDown", "Decrease display brightness", "swayosd-client --monitor $(hyprctl activeworkspace -j | jq -r .monitor) --brightness -5", { locked = true, repeating = true })

-------------------
---- UTILITIES ----
-------------------

group = "System"

-- Screen Capture
bind(mainMod .. " + P",            "Pick a colour from the screen",      "hyprpicker -a -n")
bind("Print",                      "Screenshot a region to the clipboard", (os.getenv("HOME") or "") .. "/.local/bin/garage-screenshot-copy region")
bind(mainMod .. " + Print",        "Screenshot the display to the clipboard", (os.getenv("HOME") or "") .. "/.local/bin/garage-screenshot-copy monitor")
bind(mainMod .. " + SHIFT + S",    "Open the screenshot tool",           "qs ipc --config garage call shell screenshot")
bind(mainMod .. " + Escape",       "Open the session menu",              "qs ipc --config garage call shell session")
-- Non-consuming: the click still reaches the window underneath, this only tells
-- the shell to close an open system menu.
bind("mouse:272", "Dismiss the open system menu", (os.getenv("HOME") or "") .. "/.local/bin/garage-menu-dismiss", { non_consuming = true })
bind("mouse:273", "Dismiss the open system menu", (os.getenv("HOME") or "") .. "/.local/bin/garage-menu-dismiss", { non_consuming = true })

-- The Wallpaper pane, not the standalone picker script: the wallpaper is now a
-- preference per appearance, and the script wrote the `current` symlink behind
-- preferences.toml's back -- so the next light/dark switch reverted whatever it
-- had set.
bind(mainMod .. " + SHIFT + W", "Choose a wallpaper", "qs ipc --config garage call shell preferencesOn wallpaper")

-- Clipboard
bind(mainMod .. " + SHIFT + V", "Paste from clipboard history", "bash -c 'cliphist list | rofi -normal-window -dmenu -p \"\" | cliphist decode | wl-copy'")

-- Notifications
bind(mainMod .. " + A",             "Toggle the notification panel", "swaync-client --toggle-panel")
bind(mainMod .. " + SHIFT + A",     "Show About This System",        "qs ipc --config garage call shell about")
bind(mainMod .. " + SHIFT + comma", "Open System Preferences",       "qs ipc --config garage call shell preferences")

-------------------------------
---- WORKSPACES & MONITORS ----
-------------------------------

group = "Workspaces"

-- The number keys, generated from the same WORKSPACE_PLAN config.workspaces
-- turns into rules. Reading it here rather than repeating the counts is what
-- keeps a key from addressing a workspace the rules never created.
local shared_workspaces = WORKSPACE_PLAN.mode == "shared"
local slots_per_display = {}
local slot_count = 0
-- The smallest display decides which slots exist everywhere, which is the only
-- thing separating a key that always works from one that is silent on some
-- displays. The descriptions below have to say which, since that is all a
-- settings list can show of a bind.
local slots_everywhere = math.huge
for _, group in ipairs(WORKSPACE_PLAN.groups) do
    slots_per_display[group.monitor] = group.count
    slot_count = math.max(slot_count, group.count)
    slots_everywhere = math.min(slots_everywhere, group.count)
end

-- Shared workspaces are not pinned, so a key means the workspace with that id
-- wherever it currently is. Per-display ones are addressed as the display's own
-- nth (`m~`), which is what keeps SUPER+3 meaning "my third workspace" on every
-- screen instead of naming a fixed global id.
local function workspace_for(local_slot)
    return shared_workspaces and tostring(local_slot) or ("m~" .. local_slot)
end

-- A slot only exists on the displays configured with at least that many
-- workspaces. On a display with fewer, the key does nothing rather than
-- creating an extra workspace there, which would sit outside every range the
-- rules hand out and shift nothing back into place on the next reload.
local function within_slot(local_slot, dispatcher)
    if shared_workspaces then
        return dispatcher
    end
    return function()
        local monitor = hl.get_active_monitor()
        if monitor ~= nil and (slots_per_display[monitor.name] or 0) >= local_slot then
            hl.dispatch(dispatcher)
        end
    end
end

for local_slot = 1, slot_count do
    -- Slot 10 sits on the 0 key, which is where the row runs out.
    local key = local_slot % 10
    local scope = ""
    if not shared_workspaces then
        scope = local_slot <= slots_everywhere and " on this display"
            or " (only on displays with that many)"
    end
    local switch = "Switch to workspace " .. local_slot .. scope
    local move = "Move the window to workspace " .. local_slot .. scope

    bind(mainMod .. " + " .. key, switch,
        within_slot(local_slot, hl.dsp.focus({ workspace = workspace_for(local_slot) })))
    bind(mainMod .. " + SHIFT + " .. key, move,
        within_slot(local_slot, hl.dsp.window.move({ workspace = workspace_for(local_slot) })))
    -- Alias of the line above; same action, kept for the muscle memory.
    bind(mainMod .. " + SHIFT + CONTROL + " .. key, move,
        within_slot(local_slot, hl.dsp.window.move({ workspace = workspace_for(local_slot) })))
end

-- Move to adjacent workspaces and next empty on a given monitor
bind("CONTROL + Left",                  "Switch to the previous workspace",       hl.dsp.focus({ workspace = "m-1" }))
bind("CONTROL + Right",                 "Switch to the next workspace",           hl.dsp.focus({ workspace = "m+1" }))
bind(mainMod .. " + CONTROL + Right",   "Switch to the next workspace",           hl.dsp.focus({ workspace = "m+1" }))
bind(mainMod .. " + CONTROL + Left",    "Switch to the previous workspace",       hl.dsp.focus({ workspace = "m-1" }))
bind(mainMod .. " + CONTROL + Down",    "Switch to the next empty workspace",     hl.dsp.focus({ workspace = "emptym" }))

-- Send the whole workspace to another display, rather than one window at a time.
-- Up/down means previous/next display here, matching SUPER+SHIFT+scroll above.
bind(mainMod .. " + CONTROL + SHIFT + Up",   "Move this workspace to the previous display", hl.dsp.workspace.move({ monitor = "-1" }))
bind(mainMod .. " + CONTROL + SHIFT + Down", "Move this workspace to the next display",     hl.dsp.workspace.move({ monitor = "+1" }))

-- Scroll through existing workspaces & monitors
bind(mainMod .. " + mouse_down",           "Switch to the previous workspace", hl.dsp.focus({ workspace = "m-1" }))
bind(mainMod .. " + mouse_up",             "Switch to the next workspace",     hl.dsp.focus({ workspace = "m+1" }))
bind(mainMod .. " + CONTROL + mouse_up",   "Switch to the previous workspace", hl.dsp.focus({ workspace = "m-1" }))
bind(mainMod .. " + CONTROL + mouse_down", "Switch to the next workspace",     hl.dsp.focus({ workspace = "m+1" }))

-- Special workspace (scratchpad)
bind(mainMod .. " + SHIFT + grave", "Move the window to the scratchpad", hl.dsp.window.move({ workspace = "special" }))
bind(mainMod .. " + S",             "Toggle the scratchpad",             hl.dsp.workspace.toggle_special())

-----------------------------
---- USER-DEFINED BINDS -----
-----------------------------

-- Shortcuts the user invented in System Preferences, running whatever command
-- they typed. Not through bind() above: these have no default and no place in
-- the catalog -- the helper that wrote them already knows them -- and they must
-- never be treated as something an override could move a second time.
--
-- The command reaches /bin/sh through hl.dsp.exec_cmd as a single Lua string
-- that came out of the reader intact. It is never concatenated into this chunk,
-- so quotes, newlines and a stray `")) ;` are characters in a command rather
-- than syntax in a config.
for _, item in ipairs(custom_binds) do
    if type(item) == "table" and type(item.keys) == "string" and item.keys ~= ""
            and type(item.command) == "string" and item.command ~= "" then
        local description = type(item.description) == "string" and item.description or ""
        hl.bind(item.keys, hl.dsp.exec_cmd(item.command),
            { description = description ~= "" and description or item.command })
    end
end

-- Publish the defaults, last, so the list is everything above rather than
-- whatever had been declared at some earlier point in the file. Tab separated
-- and unescaped on purpose: every field here was written in this file, and none
-- of them contains a tab or a newline. Nothing the user typed passes through.
--
-- Written to a temporary and renamed into place, never straight over the file
-- the helper reads. A plain io.open(..., "w") truncates before it writes, so a
-- `garage` process arriving mid-write saw a leading fragment of this list --
-- which looks exactly like a complete but shorter catalog. The helper filtered
-- the user's overrides against it and wrote the survivors back over
-- keybindings.toml, so changing one shortcut during a reload could silently
-- delete every rebind whose bind had not been written yet. rename(2) is atomic
-- within one filesystem and generated/ is a single directory, so a reader now
-- sees either the previous catalog or this one, whole.
--
-- The last line is the witness the helper checks: "#end<TAB>N", where N is the
-- number of rows above it. Two fields, so the helper's five-field parse skips it
-- as data rather than reading it as a bind, and it is absent or short in exactly
-- the case a file is a fragment. A catalog without it is treated as unverified
-- rather than as an error -- which is what an older session's copy of this file
-- still running against a newer helper produces.
local rows = #catalog
catalog[rows + 1] = "#end\t" .. tostring(rows)
local catalog_body = table.concat(catalog, "\n") .. "\n"

local function write_catalog(path)
    local file = io.open(path, "w")
    if file == nil then
        return false
    end
    local wrote = file:write(catalog_body)
    file:close()
    return wrote and true or false
end

-- Best effort, and the fallback matters as much as the atomic path. generated/
-- belongs to the helper and does not exist on a first login until it has run
-- once, and os.rename is a standard-library call this file cannot prove is
-- reachable inside Hyprland's Lua sandbox -- os.getenv is the only os.* function
-- the rest of this config tree uses. pcall covers both a sandbox that does not
-- expose it (calling nil raises) and a rename that merely fails (nil plus a
-- message). Either way the old direct write is used instead: a torn catalog
-- becomes possible again, but the witness above keeps the helper from acting on
-- one, and a desktop whose shortcuts work but cannot yet be listed is a far
-- better failure than the other way round.
local temporary = CATALOG_FILE .. ".new"
local published = false
if write_catalog(temporary) then
    local called, renamed = pcall(os.rename, temporary, CATALOG_FILE)
    published = (called and renamed) and true or false
    if not published then
        pcall(os.remove, temporary)
    end
end
if not published then
    write_catalog(CATALOG_FILE)
end
