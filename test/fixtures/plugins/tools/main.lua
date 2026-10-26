-- Test plugin (H3.3b): files and a process in the trusted project.
ferny.command {
    id = "go",
    title = "Go",
    run = function()
        local list = ferny.fs.list(".")
        local written = ferny.fs.write("plugin-out.txt", "written by a plugin")
        ferny.spawn {
            program = "echo",
            args = { "spawned" },
            on_exit = function(r)
                ferny.ui.status("exit " .. tostring(r.code) .. " " .. r.stdout:gsub("%s+$", ""))
            end,
        }
        ferny.ui.status("listed " .. tostring(list and #list) .. " written " .. tostring(written))
    end,
}
