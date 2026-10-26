-- Auto-Mark (ferny-automark): resalta todas las ocurrencias de la palabra bajo el cursor.
-- Basado en el plugin "automark" de Geany-Plugins (versión 2.1), escrito en C por Pavel Roschin.
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

local last_word = nil
local last_line = nil

-- Buscar todas las ocurrencias de una palabra en el texto
local function find_word_occurrences(text, word)
    local occurrences = {}
    local pattern = "%b" .. word .. ""

    -- Usar match para encontrar todas las ocurrencias
    local start_pos = 1
    while start_pos <= #text do
        local found = text:sub(start_pos):find(word, 1, true)
        if not found then
            break
        end
        local abs_pos = start_pos + found - 1
        -- Verificar que es una palabra completa (whole word)
        local before = abs_pos > 1 and text:sub(abs_pos - 1, abs_pos - 1) or ""
        local after = abs_pos + #word <= #text and text:sub(abs_pos + #word, abs_pos + #word) or ""
        local is_word_boundary = (before == "" or before:match("%s") or before:match("[^%w_]")) and
                                 (after == "" or after:match("%s") or after:match("[^%w_]"))
        if is_word_boundary then
            table.insert(occurrences, abs_pos)
        end
        start_pos = abs_pos + 1
    end

    return occurrences
end

-- Obtener la palabra actual en la posición del cursor
local function get_current_word()
    local doc = ferny.documents.current()
    if not doc then
        return nil, nil
    end

    local text = ferny.editor.text()
    local line = doc.line
    local column = doc.column

    -- Encontrar los límites de la palabra
    local line_text = ""
    local line_num = 1
    for l in text:gmatch("([^\n]*)\n?") do
        if line_num == line then
            line_text = l
            break
        end
        line_num = line_num + 1
    end

    if line_text == "" then
        return nil, nil
    end

    local col = column
    -- Buscar inicio de palabra
    local word_start = col
    while word_start > 1 do
        local prev = line_text:sub(word_start - 1, word_start - 1)
        if prev:match("[%w_]") then
            word_start = word_start - 1
        else
            break
        end
    end

    -- Buscar fin de palabra
    local word_end = col
    while word_end < #line_text do
        local next = line_text:sub(word_end + 1, word_end + 1)
        if next:match("[%w_]") then
            word_end = word_end + 1
        else
            break
        end
    end

    local word = line_text:sub(word_start, word_end)

    -- Filtrar palabras muy cortas o muy largas
    if #word < 2 or #word > 30 then
        return nil, nil
    end

    return word, {line = line, start = word_start, end = word_end}
end

-- Comando para marcar/desmarcar manualmente
ferny.command {
    id = "mark-word",
    title = ferny.tr("Mark Word Occurrences"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        local word, pos = get_current_word()
        if not word then
            ferny.ui.status(ferny.tr("Place cursor on a word first"))
            return
        end

        -- En Ferny no hay API directa para indicadores de búsqueda
        -- Esto es un placeholder para cuando la API lo soporte
        ferny.ui.status(ferny.tr("Marking '") .. word .. ferny.tr("' (API pending)"))
        ferny.log("Automark: marking word '" .. word .. "' at line " .. pos.line)
    end,
}

-- Comando para limpiar marcadores
ferny.command {
    id = "clear-markers",
    title = ferny.tr("Clear Word Markers"),
    run = function()
        last_word = nil
        last_line = nil
        ferny.ui.status(ferny.tr("Word markers cleared"))
    end,
}

-- Listener para cambios de documento
ferny.on("document.activated", function(doc)
    last_word = nil
    last_line = nil
end)

ferny.on("document.closed", function(doc)
    last_word = nil
    last_line = nil
end)

ferny.log("Automark plugin loaded (placeholder - marker API pending)")