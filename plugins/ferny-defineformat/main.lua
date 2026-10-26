-- Define Format (ferny-defineformat): formato de definiciones y macros.
-- Basado en el plugin "defineformat" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

ferny.command {
    id = "format-defines",
    title = ferny.tr("Format Defines/Macros"),
    run = function()
        local text = ferny.editor.text()
        local lines = {}

        -- Normalizar definiciones #define
        for line in text:gmatch("([^\n]*)\n?") do
            -- Expandir tabs, alinear
            local formatted = line:gsub("^%s*#define%s+(%S+)%s+(.*)$", function(name, value)
                return string.format("#define %-30s %s", name, value)
            end)
            table.insert(lines, formatted)
        end

        ferny.editor.replace_selection(table.concat(lines, "\n"))
        ferny.ui.status(ferny.tr("Definitions formatted"))
    end,
}

ferny.command {
    id = "align-equals",
    title = ferny.tr("Align = in Assignments"),
    run = function()
        local text = ferny.editor.text()
        local lines = {}
        local max_eq_pos = 0

        -- Primera pasada: encontrar posición máxima de =
        for line in text:gmatch("([^\n]*)\n?") do
            local eq_pos = line:find("=")
            if eq_pos and eq_pos > max_eq_pos then
                max_eq_pos = eq_pos
            end
        end

        -- Segunda pasada: alinear
        for line in text:gmatch("([^\n]*)\n?") do
            local eq_pos = line:find("=")
            if eq_pos and eq_pos < max_eq_pos then
                local before = line:sub(1, eq_pos - 1):gsub("%s+$", "")
                local after = line:sub(eq_pos):gsub("^%s*", "")
                table.insert(lines, before .. string.rep(" ", max_eq_pos - #before) .. " " .. after)
            else
                table.insert(lines, line)
            end
        end

        ferny.editor.replace_selection(table.concat(lines, "\n"))
        ferny.ui.status(ferny.tr("Assignments aligned"))
    end,
}

ferny.log("Define Format plugin loaded")