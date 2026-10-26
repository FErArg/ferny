-- Pair Tag Highlighter (ferny-pairtaghighlighter): resaltar etiquetas HTML/XML coincidentes.
-- Basado en el plugin "pairtaghighlighter" de Geany-Plugins (versión 2.1).
-- Adaptado para Ferny usando la API de plugins Lua (API 0).

-- Nota: Ferny no expone API para indicadores de resaltado.
-- Este plugin proporciona comandos de búsqueda manual.

local function find_matching_tag(text, pos)
    -- Buscar hacia adelante la etiqueta de cierre
    -- Implementación simplificada
    return nil
end

ferny.command {
    id = "highlight-pair-tag",
    title = ferny.tr("Highlight Matching Tag"),
    run = function()
        local doc = ferny.documents.current()
        if not doc then
            ferny.ui.status(ferny.tr("No document open"))
            return
        end

        local text = ferny.editor.text()
        local column = doc.column

        -- Buscar etiqueta actual
        local before = text:sub(1, column)
        local after = text:sub(column + 1)

        -- Buscar <tag> o </tag> antes del cursor
        local tag_start = before:match("<(%w+)%s*[^>]*>%s*$")
        if not tag_start then
            tag_start = before:match("</(%w+)>%s*$")
        end

        if tag_start then
            ferny.ui.status(ferny.tr("Tag: ") .. tag_start .. ferny.tr(" (matching API pending)"))
        else
            ferny.ui.status(ferny.tr("No tag at cursor position"))
        end
    end,
}

ferny.command {
    id = "jump-to-matching-tag",
    title = ferny.tr("Jump to Matching Tag"),
    run = function()
        ferny.ui.status(ferny.tr("Jump to matching tag - requires editor API"))
    end,
}

ferny.log("Pair Tag Highlighter plugin loaded (placeholder)")