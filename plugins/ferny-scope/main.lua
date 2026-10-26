-- Scope (ferny-scope): navegación por ámbitos/bloques.
-- Basado en el plugin "scope" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Navegación entre bloques (funciones, clases, ifs, loops)

local function find_block_start(text, line_num)
    local lines = {}
    for l in text:gmatch("([^\n]*)\n?") do
        table.insert(lines, l)
    end

    local indent_stack = {}

    for i = line_num - 1, 1, -1 do
        local line = lines[i]
        local trimmed = line:match("^%s*(.-)%s*$") or ""

        if trimmed ~= "" then
            local indent = #line - #trimmed

            -- Detectar inicio de bloque
            if trimmed:match("^function") or trimmed:match("^def ") or trimmed:match("^class ") or
               trimmed:match("^if") or trimmed:match("^for") or trimmed:match("^while") or
               trimmed:match("^try:") or trimmed:match("^with ") then
                return i
            end

            -- Detectar llaves
            local open_braces = 0
            local close_braces = 0
            for _ in line:gmatch("{") do open_braces = open_braces + 1 end
            for _ in line:gmatch("}") do close_braces = close_braces + 1 end

            if close_braces > open_braces then
                -- Más cierres que aperturas, retroceder
                local diff = close_braces - open_braces
                for _ = 1, diff do
                    if #indent_stack > 0 then
                        table.remove(indent_stack)
                    end
                end
            elseif open_braces > close_braces then
                for _ = 1, open_braces - close_braces do
                    table.insert(indent_stack, "{")
                end
            end
        end
    end

    return 1
end

ferny.command {
    id = "go-to-scope-start",
    title = ferny.tr("Go to Scope Start"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        local text = ferny.editor.text()
        local start_line = find_block_start(text, doc.line)

        ferny.ui.status(ferny.tr("Scope starts at line ") .. start_line)
    end,
}

ferny.command {
    id = "go-to-scope-end",
    title = ferny.tr("Go to Scope End"),
    run = function()
        ferny.ui.status(ferny.tr("Go to scope end - not implemented"))
    end,
}

ferny.command {
    id = "select-current-scope",
    title = ferny.tr("Select Current Scope"),
    run = function()
        ferny.ui.status(ferny.tr("Select current scope - requires editor API"))
    end,
}

ferny.log("Scope plugin loaded")