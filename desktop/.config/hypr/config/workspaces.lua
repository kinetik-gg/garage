-- Workspace rules wiki https://wiki.hypr.land/Configuring/Basics/Workspace-Rules/
-- Hyprland workspace IDs are global, so in per-display mode every display owns a
-- private block of ten of them and keeps `count` of those. The block is fixed to
-- the display, so one display's count can never renumber another's workspaces --
-- which, since a window lives on the id, is what would move it to another
-- screen. WORKSPACE_PLAN holds the blocks; see config.variables for where it
-- comes from. Only the slots below are created, so the rest of a block does not
-- exist and nothing -- number key or monitor-relative cycling -- can reach it.
--
-- Shared mode writes no numeric rule at all, and that is the whole of the
-- difference: a workspace then opens on whichever display asks for it and stays
-- where it is put, which is Hyprland with no rules -- its own default.
-- Persistence is deliberately not offered there. A persistent workspace with no
-- monitor is re-created on the focused display at every reload, so a workspace
-- carried to another display would snap back the next time the config was
-- touched, which is the opposite of what shared is asking for.
if WORKSPACE_PLAN.mode ~= "shared" then
    for _, group in ipairs(WORKSPACE_PLAN.groups) do
        for local_slot = 1, group.count do
            hl.workspace_rule({
                workspace = tostring(group.first + local_slot - 1),
                monitor = group.monitor,
                default = local_slot == 1,
                persistent = true,
            })
        end
    end
end

hl.workspace_rule({ workspace = "name:gaming", monitor = PRIMARY_MONITOR })
