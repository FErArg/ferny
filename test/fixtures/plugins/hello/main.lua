-- Test plugin (H3.2, H3.3a): logs, reacts to opened files, adds a command.
ferny.log("hello from plugin, clipboard=" .. tostring(ferny.has_permission("clipboard")))

ferny.on("document.opened", function(doc)
    ferny.ui.status("hello: opened " .. doc.filetype)
end)

ferny.command {
    id = "stamp",
    title = ferny.tr("Stamp"),
    run = function()
        local doc = ferny.documents.current()
        ferny.editor.insert(ferny.tr("Hello") .. " " .. doc.line)
        ferny.ui.status("stamped")
    end,
}

-- Extension points (H3.3c) for C++.
ferny.provide("completion", {
    filetypes = { "cpp" },
    run = function(ctx) return { ctx.word .. "_from_plugin" } end,
})
ferny.provide("hover", {
    filetypes = { "cpp" },
    run = function(ctx) return "plugin hover " .. ctx.word end,
})
ferny.provide("format", {
    filetypes = { "cpp" },
    run = function(ctx) return ctx.text .. "// formatted\n" end,
})
ferny.on("document.before_save", function(doc)
    if doc.filetype == "cpp" then
        return (doc.text:gsub("TODO", "DONE"))
    end
end)
