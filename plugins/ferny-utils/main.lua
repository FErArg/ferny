-- Utils (ferny-utils): utilidades varias.
-- Basado en el plugin "utils" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

ferny.command {
    id = "count-words",
    title = ferny.tr("Count Words"),
    run = function()
        local text = ferny.editor.text()
        local count = 0
        for _ in text:gmatch("%S+") do
            count = count + 1
        end
        ferny.ui.status(ferny.tr("Words: ") .. count)
    end,
}

ferny.command {
    id = "count-lines",
    title = ferny.tr("Count Lines"),
    run = function()
        local text = ferny.editor.text()
        local count = 1
        for _ in text:gmatch("\n") do
            count = count + 1
        end
        ferny.ui.status(ferny.tr("Lines: ") .. count)
    end,
}

ferny.command {
    id = "count-chars",
    title = ferny.tr("Count Characters"),
    run = function()
        local text = ferny.editor.text()
        ferny.ui.status(ferny.tr("Characters: ") .. #text)
    end,
}

ferny.command {
    id = "uppercase-selection",
    title = ferny.tr("Selection to Uppercase"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection(sel:upper())
    end,
}

ferny.command {
    id = "lowercase-selection",
    title = ferny.tr("Selection to Lowercase"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection(sel:lower())
    end,
}

ferny.command {
    id = "capitalize-selection",
    title = ferny.tr("Capitalize Selection"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection(sel:gsub("(%w)([%w]*)", function(first, rest)
            return first:upper() .. rest:lower()
        end))
    end,
}

ferny.command {
    id = "reverse-selection",
    title = ferny.tr("Reverse Selection"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection(sel:reverse())
    end,
}

ferny.command {
    id = "trim-selection",
    title = ferny.tr("Trim Selection"),
    run = function()
        local sel = ferny.editor.selection()
        if sel == "" then
            ferny.ui.status(ferny.tr("No text selected"))
            return
        end
        ferny.editor.replace_selection(sel:match("^%s*(.-)%s*$"))
    end,
}

ferny.log("Utils plugin loaded")