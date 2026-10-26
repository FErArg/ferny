-- Pretty Printer (ferny-pretty-printer): formatear código.
-- Basado en el plugin "pretty-printer" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Formateo simple de código (indentation, espaciado)
local function format_code(text, filetype)
    local lines = {}

    -- Configuración por defecto
    local indent_size = 4
    local use_tabs = false
    local indent_char = use_tabs and "\t" or string.rep(" ", indent_size)

    local indent_level = 0
    local in_multiline = false

    for line in text:gmatch("([^\n]*)\n?") do
        local trimmed = line:match("^%s*(.-)%s*$") or ""

        if trimmed ~= "" then
            -- Reducir indentación para cierres
            if trimmed:match("^}") or trimmed:match("^end") or trimmed:match("^else") or trimmed:match("^elif") or trimmed:match("^catch") or trimmed:match("^finally") then
                indent_level = math.max(0, indent_level - 1)
            end

            -- Aplicar indentación
            local new_line = string.rep(indent_char, indent_level) .. trimmed
            table.insert(lines, new_line)

            -- Aumentar indentación para aperturas
            if trimmed:match("{[^}]*$") or trimmed:match("^function") or trimmed:match("^if") or trimmed:match("^for") or trimmed:match("^while") or trimmed:match("^def ") or trimmed:match("^class ") or trimmed:match("^try:") or trimmed:match("^with ") then
                indent_level = indent_level + 1
            end
        else
            -- Línea vacía
            table.insert(lines, "")
        end
    end

    return table.concat(lines, "\n")
end

ferny.command {
    id = "pretty-print",
    title = ferny.tr("Pretty Print (Format Code)"),
    run = function()
        local text = ferny.editor.text()
        local doc = ferny.documents.current()
        local filetype = doc and doc.filetype or "unknown"

        local formatted = format_code(text, filetype)
        ferny.editor.replace_selection(formatted)
        ferny.ui.status(ferny.tr("Code formatted"))
    end,
}

ferny.command {
    id = "indent-selection",
    title = ferny.tr("Indent Selection"),
    run = function()
        local text = ferny.editor.text()
        local lines = {}

        for line in text:gmatch("([^\n]*)\n?") do
            table.insert(lines, "    " .. line)
        end

        ferny.editor.replace_selection(table.concat(lines, "\n"))
        ferny.ui.status(ferny.tr("Selection indented"))
    end,
}

ferny.command {
    id = "unindent-selection",
    title = ferny.tr("Unindent Selection"),
    run = function()
        local text = ferny.editor.text()
        local lines = {}

        for line in text:gmatch("([^\n]*)\n?") do
            local unindented = line:gsub("^    ", ""):gsub("^\t", "")
            table.insert(lines, unindented)
        end

        ferny.editor.replace_selection(table.concat(lines, "\n"))
        ferny.ui.status(ferny.tr("Selection unindented"))
    end,
}

ferny.log("Pretty Printer plugin loaded")