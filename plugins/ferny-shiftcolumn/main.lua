-- Shift Column (ferny-shiftcolumn): mover texto entre columnas.
-- Basado en el plugin "shiftcolumn" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- ============================================================
-- Funciones auxiliares
-- ============================================================

local function get_column_at_position(text, line, column)
    -- Encontrar la línea
    local line_num = 1
    local line_start = 1
    for l in text:gmatch("([^\n]*)\n?") do
        if line_num == line then
            break
        end
        line_start = line_start + #l + 1
        line_num = line_num + 1
    end

    -- Columna es 1-based
    local col = math.min(column, #text - line_start + 1)
    return col
end

-- ============================================================
-- Comandos
-- ============================================================

ferny.command {
    id = "shift-column-left",
    title = ferny.tr("Shift Column Left"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        local text = ferny.editor.text()
        local line = doc.line
        local column = doc.column

        -- Obtener carácter en la columna
        local col_idx = math.min(column, #text)
        local char = text:sub(col_idx, col_idx)

        if char == "" or char:match("%s") then
            ferny.ui.status(ferny.tr("No character at column"))
            return
        end

        -- Mover carácter a la izquierda (intercambiar con anterior)
        if col_idx > 1 then
            local prev_char = text:sub(col_idx - 1, col_idx - 1)
            local new_text = text:sub(1, col_idx - 2) .. char .. prev_char .. text:sub(col_idx + 1)
            ferny.editor.replace_selection(new_text)
            ferny.ui.status(ferny.tr("Shifted character left"))
        else
            ferny.ui.status(ferny.tr("Already at start of line"))
        end
    end,
}

ferny.command {
    id = "shift-column-right",
    title = ferny.tr("Shift Column Right"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        local text = ferny.editor.text()
        local line = doc.line
        local column = doc.column

        -- Obtener carácter en la columna
        local col_idx = math.min(column, #text)
        local char = text:sub(col_idx, col_idx)

        if char == "" or char:match("%s") then
            ferny.ui.status(ferny.tr("No character at column"))
            return
        end

        -- Mover carácter a la derecha (intercambiar con siguiente)
        if col_idx < #text then
            local next_char = text:sub(col_idx + 1, col_idx + 1)
            local new_text = text:sub(1, col_idx - 1) .. next_char .. char .. text:sub(col_idx + 2)
            ferny.editor.replace_selection(new_text)
            ferny.ui.status(ferny.tr("Shifted character right"))
        else
            ferny.ui.status(ferny.tr("Already at end of line"))
        end
    end,
}

ferny.command {
    id = "swap-column",
    title = ferny.tr("Swap Columns"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        local text = ferny.editor.text()
        local line = doc.line
        local column = doc.column

        -- Intercambiar carácter actual con el de la columna anterior
        if column > 1 then
            local col_idx = math.min(column, #text)
            if col_idx > 1 and col_idx <= #text then
                local prev = text:sub(col_idx - 1, col_idx - 1)
                local curr = text:sub(col_idx, col_idx)
                local new_text = text:sub(1, col_idx - 2) .. curr .. prev .. text:sub(col_idx + 1)
                ferny.editor.replace_selection(new_text)
                ferny.ui.status(ferny.tr("Swapped characters"))
            end
        else
            ferny.ui.status(ferny.tr("Cannot swap at start of line"))
        end
    end,
}

ferny.log("Shift Column plugin loaded")